//! Guarded preset authoring and Strategy edits, independent of execution.

use crate::{CoreAccess, CoreError, CoreResult, CoreService, Principal};
use nexus_contracts::{
    CoreStrategyPatchResponse, CoreStrategyPatchResponseValidationSummary,
    GetPresetResponse, ScaffoldPresetRequest, ScaffoldPresetResponse,
    StrategyConflictError, StrategyPatchPromptTemplateRequest, StrategyPatchStateRequest,
    StrategyPatchTransitionRequest, ValidatePresetRequest, ValidatePresetResponse,
};
use nexus_contracts::{UpdatePresetRequest, UpdatePresetResponse};
use nexus_contracts::{
    OrchestrationPresetListResponse, PresetProfileConditionalRule, PresetProfileEnterAction,
    PresetProfileExitWhen, PresetProfileLabeledNext, PresetProfileLanes, PresetProfileNext,
    PresetProfileResponse, PresetProfileRole, PresetProfileSignal, PresetProfileState,
};
use nexus_contracts::local::orchestration::preset::{
    EnterAction, ExitWhen, NextTarget, PresetRoleDefinition, SignalActionKind,
    SignalBinding, StateDefinition,
};
use nexus_preset::preset_ids::cron_role_preset_ids;
use nexus_contracts::generated::daemon_api::preset_management::list_presets_response::{
    ListPresetsResponse, NexusPresetSummary, NexusPresetSummarySource,
};
use nexus_preset::capability_catalog::BuiltinCapabilityCatalog;
use nexus_home_layout::{user_preset_base_dir, user_preset_bundle_dir};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Preset domain failures. Adapters retain their existing status/envelope mapping.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PresetError {
    #[error("{code}: {message}")]
    Rejected { code: String, message: String },
    #[error("invalid input: {field}: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("forbidden: {resource}: {reason}")]
    Forbidden { resource: String, reason: String },
    #[error("{code}: {message}")]
    Internal { code: String, message: String },
    #[error("strategy conflict")]
    StrategyConflict(StrategyConflictError),
    #[error("strategy validation failed")]
    StrategyValidation(CoreStrategyPatchResponseValidationSummary),
}

impl PresetError {
    fn strategy_conflict(current_revision: u64, node_id: &str, conflicting_path: &str, recovery_hint: &str) -> Self {
        Self::StrategyConflict(StrategyConflictError {
            current_revision,
            node_id: node_id.to_string(),
            conflicting_path: conflicting_path.to_string(),
            recovery_hint: recovery_hint.to_string(),
        })
    }

    fn strategy_validation_failed(errors: &[String], warnings: &[String]) -> Self {
        Self::StrategyValidation(CoreStrategyPatchResponseValidationSummary {
            errors: errors.to_vec(), warnings: warnings.to_vec(),
        })
    }
}

fn raw_user_home(nexus_home: &Path) -> Result<&Path, PresetError> {
    nexus_home.parent().ok_or_else(|| PresetError::Internal {
        code: "HOME_PATH_ERROR".into(), message: "Nexus home has no parent".into(),
    })
}

fn preset_io(code: &str, error: impl std::fmt::Display) -> PresetError {
    PresetError::Internal { code: code.to_string(), message: error.to_string() }
}

/// Resolve existing ancestors before any file effect, including a new template
/// below a symlinked parent. Lexical traversal is rejected by the caller.
fn confined_path(root: &Path, path: &Path) -> Result<PathBuf, PresetError> {
    let canonical_root = root.canonicalize().map_err(|e| preset_io("PATH_CANONICALIZE_ERROR", e))?;
    let mut ancestor = path;
    let mut missing = Vec::new();
    loop {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(ancestor.file_name().ok_or_else(|| preset_io("PATH_INVALID", "missing file name"))?);
                ancestor = ancestor.parent().ok_or_else(|| preset_io("PATH_INVALID", "missing parent"))?;
            }
            Err(e) => return Err(preset_io("PATH_METADATA_ERROR", e)),
        }
    }
    let mut resolved = ancestor.canonicalize().map_err(|e| preset_io("PATH_CANONICALIZE_ERROR", e))?;
    if !resolved.starts_with(&canonical_root) {
        return Err(PresetError::Forbidden {
            resource: "preset_path".into(), reason: "path resolves outside the preset root".into(),
        });
    }
    for component in missing.into_iter().rev() { resolved.push(component); }
    Ok(resolved)
}

/// Stable per-ID lock outside the bundle: deleting/recreating a bundle cannot
/// split concurrent writers across different lock-file inodes.
fn acquire_preset_lock(nexus_home: &Path, id: &str) -> Result<std::fs::File, PresetError> {
    validate_strategy_id(id)?;
    let root = confined_path(nexus_home, &nexus_home.join("presets"))?;
    std::fs::create_dir_all(&root).map_err(|e| preset_io("DIR_CREATE_ERROR", e))?;
    let lock_dir = confined_path(&root, &root.join(".locks"))?;
    std::fs::create_dir_all(&lock_dir).map_err(|e| preset_io("DIR_CREATE_ERROR", e))?;
    let lock_path = confined_path(&root, &lock_dir.join(format!("{id}.lock")))?;
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .open(lock_path).map_err(|e| preset_io("LOCK_OPEN_ERROR", e))?;
    file.lock().map_err(|e| preset_io("LOCK_ACQUIRE_ERROR", e))?;
    Ok(file)
}

impl CoreService {
    async fn preset_write<T: Send + 'static>(
        &self, principal: &Principal, id: String,
        operation: impl FnOnce(&Path, &str) -> Result<T, PresetError> + Send + 'static,
    ) -> CoreResult<T> {
        self.verify_principal(principal)?;
        if self.inner.access == CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden { resource: "preset authoring: read-only core access".into() });
        }
        validate_strategy_id(&id).map_err(CoreError::from)?;
        let service = Self { inner: Arc::clone(&self.inner) };
        let principal = principal.clone();
        tokio::task::spawn_blocking(move || {
            let _lock = acquire_preset_lock(&service.inner.nexus_home, &id).map_err(CoreError::from)?;
            service.verify_principal(&principal)?;
            operation(&service.inner.nexus_home, &id).map_err(CoreError::from)
        }).await.map_err(|e| CoreError::Internal { category: format!("preset_task: {e}") })?
    }

    pub async fn patch_strategy_state(&self, principal: &Principal, strategy_id: String,
        state_id: String, request: StrategyPatchStateRequest) -> CoreResult<CoreStrategyPatchResponse> {
        self.verify_principal(principal)?;
        ensure_id_matches(&strategy_id, &request.strategy_id, "strategy_id").map_err(CoreError::from)?;
        ensure_id_matches(&state_id, &request.state_id, "state_id").map_err(CoreError::from)?;
        self.preset_write(principal, strategy_id, move |home, id| patch_state_inner(home, id, &state_id, &request)).await
    }

    pub async fn patch_strategy_transition(&self, principal: &Principal, strategy_id: String,
        request: StrategyPatchTransitionRequest) -> CoreResult<CoreStrategyPatchResponse> {
        self.verify_principal(principal)?;
        ensure_id_matches(&strategy_id, &request.strategy_id, "strategy_id").map_err(CoreError::from)?;
        self.preset_write(principal, strategy_id, move |home, id| patch_transition_inner(home, id, &request)).await
    }

    pub async fn patch_strategy_prompt_template(&self, principal: &Principal, strategy_id: String,
        state_id: String, request: StrategyPatchPromptTemplateRequest) -> CoreResult<CoreStrategyPatchResponse> {
        self.verify_principal(principal)?;
        ensure_id_matches(&strategy_id, &request.strategy_id, "strategy_id").map_err(CoreError::from)?;
        ensure_id_matches(&state_id, &request.state_id, "state_id").map_err(CoreError::from)?;
        self.preset_write(principal, strategy_id, move |home, id| patch_prompt_template_inner(home, id, &state_id, &request)).await
    }

    pub async fn scaffold_preset(&self, principal: &Principal, request: ScaffoldPresetRequest) -> CoreResult<ScaffoldPresetResponse> {
        self.verify_principal(principal)?;
        validate_strategy_id(&request.name).map_err(|error| match error {
            PresetError::InvalidInput { reason, .. } => PresetError::InvalidInput {
                field: "name".into(), reason,
            },
            error => error,
        })?;
        self.preset_write(principal, request.name, scaffold_user_preset).await
    }

    pub async fn delete_preset(&self, principal: &Principal, preset_id: String) -> CoreResult<()> {
        self.preset_write(principal, preset_id, |home, id| {
            let (source, path) = locate_preset(home, id)?;
            if source != "user" {
                return Err(PresetError::Rejected { code: "preset_delete_forbidden".into(),
                    message: format!("only user presets can be deleted; '{id}' is {source}") });
            }
            let path = path.ok_or_else(|| preset_io("PRESET_PATH_MISSING", id))?;
            std::fs::remove_dir_all(path).map_err(|e| preset_io("DIRECTORY_REMOVE_ERROR", e))
        }).await
    }

    pub async fn get_preset(&self, principal: &Principal, preset_id: String) -> CoreResult<GetPresetResponse> {
        self.verify_principal(principal)?;
        let (source, path) = locate_preset(&self.inner.nexus_home, &preset_id).map_err(CoreError::from)?;
        let yaml = load_preset_yaml(&self.inner.nexus_home, &preset_id, &source, path.as_deref()).map_err(CoreError::from)?;
        Ok(GetPresetResponse { id: preset_id, source: source.parse().map_err(|e| CoreError::Internal { category: format!("preset_source: {e}") })?,
            path: path.map(|p| p.display().to_string()), yaml })
    }

    pub async fn list_presets(&self, principal: &Principal) -> CoreResult<ListPresetsResponse> {
        self.verify_principal(principal)?;
        let caps = BuiltinCapabilityCatalog;
        let embedded = nexus_preset::list_embedded_presets().into_iter().map(|id| {
            let run_intents = nexus_preset::load_embedded_preset(&id, &caps).ok().map_or_else(Vec::new, |loaded| {
                loaded.manifest.preset.run_intents.iter().map(|intent| {
                    use nexus_preset::manifest::RunIntent;
                    match intent {
                        RunIntent::WorkInit => "work_init",
                        RunIntent::WorkContinue => "work_continue",
                        RunIntent::KnowledgeIngest => "knowledge_ingest",
                        RunIntent::WorkMaintenance => "work_maintenance",
                        RunIntent::SystemMaintenance => "system_maintenance",
                    }.to_string()
                }).collect()
            });
            NexusPresetSummary { id, source: NexusPresetSummarySource::Embedded, run_intents }
        }).collect();
        let system = nexus_preset::system_preset_dir::scan_system_presets(&self.inner.nexus_home, &caps)
            .presets.into_iter().map(|entry| NexusPresetSummary { id: entry.qualified_id, source: NexusPresetSummarySource::System, run_intents: vec![] }).collect();
        let user = nexus_home_layout::list_user_preset_ids(raw_user_home(&self.inner.nexus_home).map_err(CoreError::from)?)
            .into_iter().map(|id| NexusPresetSummary { id, source: NexusPresetSummarySource::User, run_intents: vec![] }).collect();
        Ok(ListPresetsResponse { embedded, system, user })
    }

    pub async fn validate_preset(&self, principal: &Principal, request: ValidatePresetRequest) -> CoreResult<ValidatePresetResponse> {
        self.verify_principal(principal)?;
        validate_preset_file(&request).map_err(CoreError::from)
    }

    /// Replace user YAML without changing the retained request or revision policy.
    pub async fn update_preset(
        &self, principal: &Principal, preset_id: String, request: UpdatePresetRequest,
    ) -> CoreResult<UpdatePresetResponse> {
        self.preset_write(principal, preset_id, move |home, id| {
            let (source, path) = locate_preset(home, id)?;
            if source != "user" {
                return Err(PresetError::Rejected {
                    code: "preset_update_forbidden".into(),
                    message: format!("only user presets can be updated; '{id}' is {source}"),
                });
            }
            parse_and_check_manifest(&request.yaml)?;
            let dir = path.ok_or_else(|| preset_io("PRESET_PATH_MISSING", id))?;
            let target = confined_path(&dir, &dir.join("preset.yaml"))?;
            let temporary = dir.join(format!(".preset.yaml.{}.tmp", uuid::Uuid::new_v4()));
            atomic_write_with_dir_fsync(&target, &temporary, request.yaml.as_bytes())
                .map_err(|error| preset_io("FILE_WRITE_ERROR", error))?;
            Ok(UpdatePresetResponse { id: id.to_string(), updated: true })
        }).await
    }

    /// Retained orchestration listing: embedded IDs followed by unique system IDs.
    pub async fn list_orchestration_presets(
        &self, principal: &Principal,
    ) -> CoreResult<OrchestrationPresetListResponse> {
        self.verify_principal(principal)?;
        let mut presets = nexus_preset::list_embedded_presets();
        let scan = nexus_preset::system_preset_dir::scan_system_presets(
            &self.inner.nexus_home, &BuiltinCapabilityCatalog,
        );
        for id in nexus_preset::system_preset_dir::list_system_preset_ids(&scan) {
            if !presets.contains(&id) { presets.push(id); }
        }
        Ok(OrchestrationPresetListResponse { presets })
    }

    /// Read the profile using the retained user/system/embedded resolution order.
    pub async fn get_preset_profile(
        &self, principal: &Principal, preset_id: String,
    ) -> CoreResult<PresetProfileResponse> {
        self.verify_principal(principal)?;
        validate_strategy_id(&preset_id).map_err(CoreError::from)?;
        let home = &self.inner.nexus_home;
        let caps = BuiltinCapabilityCatalog;
        let loaded = match nexus_preset::lookup_preset_by_id(&preset_id, home, &caps) {
            Some(loaded) => loaded,
            None => nexus_preset::resolve_preset(&preset_id, home, &caps)
                .map_err(|error| PresetError::NotFound(format!("preset '{preset_id}' not found: {error}")))?,
        };
        let mut hash_hex = String::with_capacity(64);
        for byte in &loaded.source_hash {
            use std::fmt::Write;
            write!(hash_hex, "{byte:02x}").expect("writing to String is infallible");
        }
        Ok(PresetProfileResponse {
            id: loaded.id,
            version: loaded.version,
            source_hash: hash_hex,
            lanes: profile_lanes(&preset_id, !is_user_preset(&preset_id, home)),
            states: loaded.manifest.states.iter().map(profile_state).collect(),
            roles: loaded.roles.iter().map(profile_role).collect(),
            required_capabilities: loaded.manifest.preset.requires_capabilities,
            signals: loaded.signals.iter().map(profile_signal).collect(),
        })
    }
}

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

fn parse_state_set(value: &Value) -> Result<StatePatchSet, PresetError> {
    let label = value
        .get("label")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);
    if label.is_none() && description.is_none() {
        return Err(PresetError::InvalidInput {
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
) -> Result<(serde_yaml::Value, std::path::PathBuf, u64), PresetError> {
    validate_strategy_id(strategy_id)?;

    // `nexus-home-layout` user-preset helpers take the RAW user home and join
    // `.nexus42` internally (conventions/nexus-home-layout-path-helpers.md);
    // passing the `.nexus42` root would double-nest to
    // `<home>/.nexus42/.nexus42/presets` (F-QA-001).
    let raw_home = raw_user_home(nexus_home)?;
    let bundle_dir = user_preset_bundle_dir(raw_home, strategy_id);
    let yaml_path = bundle_dir.join("preset.yaml");
    if strategy_id.starts_with("_system.") {
        return Err(PresetError::Rejected { code: "strategy_update_forbidden".into(), message: format!("system preset '{strategy_id}' is read-only") });
    }
    if !yaml_path.is_file() {
        // Reject embedded/system presets explicitly so callers get a clear
        // forbidden message rather than a generic not-found.
        if nexus_preset::list_embedded_presets().contains(&strategy_id.to_string()) {
            return Err(PresetError::Rejected {
                code: "strategy_update_forbidden".to_string(),
                message: format!("embedded preset '{strategy_id}' is read-only"),
            });
        }
        let system_path = user_preset_base_dir(raw_home)
            .join("_system")
            .join(strategy_id)
            .join("preset.yaml");
        if system_path.is_file() {
            return Err(PresetError::Rejected {
                code: "strategy_update_forbidden".to_string(),
                message: format!("system preset '{strategy_id}' is read-only"),
            });
        }
        return Err(PresetError::NotFound(format!(
            "Strategy '{strategy_id}' not found"
        )));
    }

    let bundle_dir = confined_path(&user_preset_base_dir(raw_home), &bundle_dir)?;
    let yaml_path = confined_path(&bundle_dir, &yaml_path)?;
    let yaml = std::fs::read_to_string(&yaml_path).map_err(|e| PresetError::Internal {
        code: "FILE_READ_ERROR".to_string(),
        message: e.to_string(),
    })?;

    if yaml.len() > PRESET_MAX_YAML_SIZE {
        return Err(PresetError::Rejected {
            code: "strategy_yaml_too_large".to_string(),
            message: format!(
                "preset YAML exceeds maximum size ({} bytes, limit is {} bytes)",
                yaml.len(),
                PRESET_MAX_YAML_SIZE
            ),
        });
    }

    let value: serde_yaml::Value =
        serde_yaml::from_str(&yaml).map_err(|e| PresetError::Rejected {
            code: "strategy_yaml_invalid".to_string(),
            message: format!("preset.yaml is not valid YAML: {e}"),
        })?;

    let depth = nexus_preset::yaml_value_depth(&value);
    if depth > PRESET_MAX_YAML_DEPTH {
        return Err(PresetError::Rejected {
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
fn validate_strategy_id(id: &str) -> Result<(), PresetError> {
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id == "."
        || id == ".."
        || id == "_system"
        || id.chars().any(char::is_control)
    {
        return Err(PresetError::InvalidInput {
            field: "strategy_id".to_string(),
            reason:
                "must be a non-empty path segment without separators, not '.', '..', or '_system'"
                    .to_string(),
        });
    }
    Ok(())
}

/// Ensure the request's repeated identifier matches the URL path.
fn ensure_id_matches(path: &str, body: &str, field: &str) -> Result<(), PresetError> {
    if path != body {
        return Err(PresetError::Rejected {
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
) -> PresetError {
    PresetError::strategy_conflict(current_revision, node_id, conflicting_path, recovery_hint)
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
) -> Result<Vec<String>, PresetError> {
    if new_id.is_empty() {
        return Err(PresetError::InvalidInput {
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
) -> Result<(Vec<String>, Vec<String>), PresetError> {
    let manifest: nexus_contracts::local::orchestration::preset::PresetManifest =
        serde_yaml::from_value(yaml_value.clone()).map_err(|e| PresetError::Rejected {
            code: "strategy_validation_failed".to_string(),
            message: format!("structural validation failed: {e}"),
        })?;

    let caps = BuiltinCapabilityCatalog;

    let mut errors: Vec<String> =
        nexus_preset::loader_validate_manifest_compat(&manifest, &caps)
            .iter()
            .map(|p| format!("{}: {}", p.path, p.error))
            .collect();

    let sem = nexus_preset::validate_preset_semantic(&manifest, &caps);
    for d in sem.errors() {
        errors.push(format!("{}: {}", d.path, d.message));
    }

    let path_result = nexus_preset::validate_path_safety(&manifest);
    let asset_result =
        nexus_preset::validate_assets_in_bundle(&manifest, bundle_root);
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
) -> Result<(), PresetError> {
    if let Some(root) = value.as_mapping_mut() {
        root.insert(
            serde_yaml::Value::String("revision".to_string()),
            serde_yaml::Value::Number(new_revision.into()),
        );
    }

    let yaml_path = bundle_root.join("preset.yaml");
    let suffix = uuid::Uuid::new_v4().to_string();
    let tmp_path = bundle_root.join(format!("preset.yaml.tmp.{suffix}"));

    let yaml_str = serde_yaml::to_string(value).map_err(|e| PresetError::Internal {
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
) -> Result<(), PresetError> {
    std::fs::write(tmp_path, content).map_err(|e| PresetError::Internal {
        code: "FILE_WRITE_ERROR".to_string(),
        message: format!("cannot write temp file {}: {e}", tmp_path.display()),
    })?;

    let file = std::fs::File::open(tmp_path).map_err(|e| PresetError::Internal {
        code: "FILE_SYNC_ERROR".to_string(),
        message: format!("cannot open temp file for fsync: {e}"),
    })?;
    file.sync_all().map_err(|e| PresetError::Internal {
        code: "FILE_SYNC_ERROR".to_string(),
        message: format!("cannot fsync temp file: {e}"),
    })?;
    drop(file);

    std::fs::rename(tmp_path, target_path).map_err(|e| {
        let _ = std::fs::remove_file(tmp_path);
        PresetError::Internal {
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
        let dir = std::fs::File::open(parent).map_err(|e| PresetError::Internal {
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
fn backup_existing_file(path: &std::path::Path) -> Result<Option<Vec<u8>>, PresetError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| PresetError::Internal {
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
fn validate_transition_condition(condition: &str) -> Result<(), PresetError> {
    nexus_preset::expr::parse(condition)
        .map(|_| ())
        .map_err(|e| PresetError::Rejected {
            code: "strategy_transition_condition_invalid".to_string(),
            message: format!("transition condition is not a valid expression: {e}"),
        })
}

/// Validate the `op` field of a transition patch request.
///
/// Only `"create"` and `"update"` are accepted; any other value (including
/// `"delete"`) is rejected with a 422 `invalid_input` error so raw clients cannot
/// silently fall through to the update path (Greptile Issue 5).
fn validate_transition_op(op: &str) -> Result<(), PresetError> {
    if op == "create" || op == "update" {
        Ok(())
    } else {
        Err(PresetError::Rejected {
            code: "invalid_input".to_string(),
            message: format!("op must be 'create' or 'update', got '{op}'"),
        })
    }
}

/// Validate the `transition_kind` field of a transition patch request.
fn validate_transition_kind(kind: &str) -> Result<(), PresetError> {
    if matches!(kind, "next" | "branch" | "default") {
        Ok(())
    } else {
        Err(PresetError::Rejected {
            code: "invalid_input".to_string(),
            message: format!(
                "transition_kind must be 'next', 'branch', or 'default', got '{kind}'"
            ),
        })
    }
}

fn patch_state_inner(
    nexus_home: &std::path::Path,
    strategy_id: &str,
    state_id: &str,
    req: &StrategyPatchStateRequest,
) -> Result<CoreStrategyPatchResponse, PresetError> {

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

    let set = parse_state_set(
        &serde_json::to_value(&req.set).expect("serializing StrategyPatchStateRequestSet"),
    )?;

    let states = yaml_value
        .get_mut("states")
        .and_then(|v| v.as_sequence_mut())
        .ok_or_else(|| PresetError::Rejected {
            code: "strategy_invalid".to_string(),
            message: "preset.yaml is missing the 'states' array".to_string(),
        })?;

    let idx = find_state_index(states, state_id).ok_or_else(|| {
        PresetError::NotFound(format!(
            "state '{state_id}' not found in Strategy '{strategy_id}'"
        ))
    })?;

    let mut side_effects: Vec<String> = Vec::new();

    if let Some(new_label) = set.label {
        let new_label = new_label.trim().to_string();
        if new_label != state_id {
            let ids = state_ids(states);
            if ids.contains(&new_label) {
                return Err(PresetError::Rejected {
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
            .ok_or_else(|| PresetError::Internal {
                code: "STRATEGY_STATES_MISSING".to_string(),
                message: "states array disappeared during description update".to_string(),
            })?;
        if let Some(state_node) = states2.get_mut(idx) {
            state_node["description"] = serde_yaml::Value::String(description);
        }
    }

    let (errors, warnings) = validate_preset_yaml(&bundle_dir, &yaml_value)?;
    if !errors.is_empty() {
        return Err(PresetError::strategy_validation_failed(
            &errors, &warnings,
        ));
    }

    let new_revision = current_revision.checked_add(1).ok_or_else(|| PresetError::Rejected { code: "strategy_revision_exhausted".into(), message: "strategy revision exhausted".into() })?;
    write_preset_yaml(&bundle_dir, &mut yaml_value, new_revision)?;

    Ok(CoreStrategyPatchResponse {
        new_revision: std::num::NonZeroU64::new(new_revision).unwrap_or(std::num::NonZeroU64::MIN),
        validation_summary: CoreStrategyPatchResponseValidationSummary { errors: vec![], warnings },
        side_effects,
    })
}

/// Apply a transition patch to a `next` YAML value.
///
/// Returns `(matched, side_effects)`. If no branch matches, `matched` is false
/// so the caller can emit a `strategy_transition_not_found` error.
fn apply_transition_patch(
    next: &mut serde_yaml::Value,
    old_target: &str,
    req: &StrategyPatchTransitionRequest,
) -> (bool, Vec<String>) {
    let mut matched = false;
    let mut side_effects: Vec<String> = Vec::new();

    if next.is_string() {
        if next.as_str().is_some_and(|v| v == old_target) {
            let new_target = req.new_target.as_deref().unwrap_or(old_target);
            *next = serde_yaml::Value::String(new_target.to_string());
            matched = true;
            side_effects.push(format!(
                "transition {} -> {} set to {}",
                req.source_state_id, old_target, new_target
            ));
        }
    } else if let Some(next_map) = next.as_mapping_mut() {
        matched = apply_conditional_rules(next_map, old_target, req, &mut side_effects);

        if !matched {
            matched = apply_default_transition(next_map, old_target, req, &mut side_effects);
        }

        if !matched {
            matched = apply_go_nogo_branches(next_map, old_target, req, &mut side_effects);
        }
    }

    (matched, side_effects)
}

/// Match and update a conditional/labeled `rules` branch inside a transition.
fn apply_conditional_rules(
    next_map: &mut serde_yaml::Mapping,
    old_target: &str,
    req: &StrategyPatchTransitionRequest,
    side_effects: &mut Vec<String>,
) -> bool {
    let mut matched = false;
    if let Some(rules) = next_map.get_mut("rules").and_then(|v| v.as_sequence_mut()) {
        // NOTE: when `req.condition` is omitted, every rule whose `to` matches
        // `old_target` is updated. This means an `op: "update"` reconnect can
        // rewrite multiple conditional branches that point to the same target
        // (Greptile Issue 1 — deferred; add disambiguation before relying on it).
        for rule in rules {
            let to_match = rule
                .get("to")
                .and_then(serde_yaml::Value::as_str)
                .is_some_and(|v| v == old_target);
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
                    req.source_state_id, old_target
                ));
            }
        }
    }
    matched
}

/// Match and update the `default` target of a conditional transition.
fn apply_default_transition(
    next_map: &mut serde_yaml::Mapping,
    old_target: &str,
    req: &StrategyPatchTransitionRequest,
    side_effects: &mut Vec<String>,
) -> bool {
    if let Some(default) = next_map.get_mut("default") {
        if default.as_str().is_some_and(|v| v == old_target) {
            if let Some(new_target) = &req.new_target {
                *default = serde_yaml::Value::String(new_target.clone());
            }
            side_effects.push(format!(
                "default transition {} -> {} updated",
                req.source_state_id, old_target
            ));
            return true;
        }
    }
    false
}

/// Match and update `go` / `nogo` branches of a transition.
fn apply_go_nogo_branches(
    next_map: &mut serde_yaml::Mapping,
    old_target: &str,
    req: &StrategyPatchTransitionRequest,
    side_effects: &mut Vec<String>,
) -> bool {
    let mut matched = false;
    for key in ["go", "nogo"] {
        if let Some(branch) = next_map.get_mut(key) {
            if branch.as_str().is_some_and(|v| v == old_target) {
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

/// Whether a conditional rule matches the same `(to, when)` pair as a create request.
fn transition_rule_matches(
    rule: &serde_yaml::Value,
    target: &str,
    condition: Option<&str>,
) -> bool {
    let to_match = rule
        .get("to")
        .and_then(serde_yaml::Value::as_str)
        .is_some_and(|v| v == target);
    if !to_match {
        return false;
    }
    condition.map_or_else(
        || rule.get("when").is_none(),
        |cond| {
            rule.get("when")
                .and_then(serde_yaml::Value::as_str)
                .is_some_and(|v| v == cond)
        },
    )
}

fn reject_linear_next_conflict(
    next: &serde_yaml::Value,
    source_state_id: &str,
) -> Result<(), PresetError> {
    if next.is_string() {
        return Err(PresetError::Rejected {
            code: "strategy_transition_already_linear".to_string(),
            message: format!(
                "state '{source_state_id}' already has a linear next transition; create a branch instead"
            ),
        });
    }
    Ok(())
}

fn ensure_conditional_next_map(
    next: &mut serde_yaml::Value,
    source_state_id: &str,
) -> Result<(), PresetError> {
    reject_linear_next_conflict(next, source_state_id)?;

    if next.is_null() {
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("kind".to_string()),
            serde_yaml::Value::String("conditional".to_string()),
        );
        *next = serde_yaml::Value::Mapping(map);
    }

    if !next.is_mapping() {
        return Err(PresetError::Rejected {
            code: "strategy_transition_invalid".to_string(),
            message: format!("state '{source_state_id}' has an unsupported next transition shape"),
        });
    }

    Ok(())
}

fn append_conditional_rule(
    next_map: &mut serde_yaml::Mapping,
    req: &StrategyPatchTransitionRequest,
    new_target: &str,
) -> Result<(), PresetError> {
    let rules = next_map
        .entry(serde_yaml::Value::String("rules".to_string()))
        .or_insert_with(|| serde_yaml::Value::Sequence(Vec::new()))
        .as_sequence_mut()
        .ok_or_else(|| PresetError::Rejected {
            code: "strategy_transition_invalid".to_string(),
            message: format!(
                "state '{}' has a non-array rules transition",
                req.source_state_id
            ),
        })?;

    if rules
        .iter()
        .any(|rule| transition_rule_matches(rule, new_target, req.condition.as_deref()))
    {
        return Err(PresetError::Rejected {
            code: "strategy_transition_duplicate".to_string(),
            message: format!(
                "state '{}' already has a transition to '{}' with the same condition",
                req.source_state_id, new_target
            ),
        });
    }

    let mut new_rule = serde_yaml::Mapping::new();
    new_rule.insert(
        serde_yaml::Value::String("to".to_string()),
        serde_yaml::Value::String(new_target.to_string()),
    );
    if let Some(condition) = &req.condition {
        new_rule.insert(
            serde_yaml::Value::String("when".to_string()),
            serde_yaml::Value::String(condition.clone()),
        );
    }
    rules.push(serde_yaml::Value::Mapping(new_rule));
    Ok(())
}

/// Resolve the create form when `transition_kind` is omitted.
///
/// Legacy callers without `transition_kind` keep the shipped behavior: absent
/// `next` becomes a linear scalar; an existing conditional map appends a rule.
fn resolved_create_transition_kind<'a>(
    req: &'a StrategyPatchTransitionRequest,
    next: &serde_yaml::Value,
) -> Result<&'a str, PresetError> {
    match req
        .transition_kind
        .as_ref()
        .map(nexus_contracts::StrategyPatchTransitionRequestTransitionKind::as_str)
    {
        Some(kind) => {
            validate_transition_kind(kind)?;
            Ok(kind)
        }
        None if next.is_null() => Ok("next"),
        None => Ok("branch"),
    }
}

/// Create a new outgoing transition on a `next` YAML value.
///
/// Honors `transition_kind` when supplied:
/// - `next` — linear scalar when `next` is absent; rejects conditional maps.
/// - `branch` — appends a conditional rule (or seeds a conditional map).
/// - `default` — sets the conditional `default` target.
fn apply_transition_create(
    next: &mut serde_yaml::Value,
    req: &StrategyPatchTransitionRequest,
    new_target: &str,
) -> Result<Vec<String>, PresetError> {
    if req.source_state_id == new_target {
        return Err(PresetError::Rejected {
            code: "strategy_self_loop".to_string(),
            message: format!(
                "state '{}' cannot transition to itself",
                req.source_state_id
            ),
        });
    }

    let kind = resolved_create_transition_kind(req, next)?;
    let mut side_effects: Vec<String> = Vec::new();

    match kind {
        "next" => {
            if !next.is_null() {
                return Err(PresetError::Rejected {
                    code: "strategy_transition_already_conditional".to_string(),
                    message: format!(
                        "state '{}' already has a conditional next transition; create a branch or default instead",
                        req.source_state_id
                    ),
                });
            }
            *next = serde_yaml::Value::String(new_target.to_string());
            side_effects.push(format!(
                "transition {} -> {} created",
                req.source_state_id, new_target
            ));
        }
        "branch" => {
            ensure_conditional_next_map(next, &req.source_state_id)?;
            let next_map = next
                .as_mapping_mut()
                .expect("ensure_conditional_next_map leaves a mapping");
            append_conditional_rule(next_map, req, new_target)?;
            // Conditional presets require a `default` target; seed one when the
            // author adds the first branch to an otherwise-empty state.
            if !next_map.contains_key(serde_yaml::Value::String("default".to_string())) {
                next_map.insert(
                    serde_yaml::Value::String("default".to_string()),
                    serde_yaml::Value::String(new_target.to_string()),
                );
            }
            side_effects.push(format!(
                "branch {} -> {} created",
                req.source_state_id, new_target
            ));
        }
        "default" => {
            if req.condition.is_some() {
                return Err(PresetError::Rejected {
                    code: "strategy_transition_default_condition".to_string(),
                    message: "default transitions do not accept a condition".to_string(),
                });
            }
            ensure_conditional_next_map(next, &req.source_state_id)?;
            let next_map = next
                .as_mapping_mut()
                .expect("ensure_conditional_next_map leaves a mapping");
            next_map.insert(
                serde_yaml::Value::String("default".to_string()),
                serde_yaml::Value::String(new_target.to_string()),
            );
            side_effects.push(format!(
                "default transition {} -> {} created",
                req.source_state_id, new_target
            ));
        }
        _ => unreachable!("resolved_create_transition_kind only returns known kinds"),
    }

    Ok(side_effects)
}
fn patch_transition_inner(
    nexus_home: &std::path::Path,
    strategy_id: &str,
    req: &StrategyPatchTransitionRequest,
) -> Result<CoreStrategyPatchResponse, PresetError> {
    // Reject unknown `op` values early so raw clients cannot send e.g. `op: "delete"`
    // and silently fall through to the update path (Greptile Issue 5).
    validate_transition_op(req.op.as_str())?;


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
        .ok_or_else(|| PresetError::Rejected {
            code: "strategy_invalid".to_string(),
            message: "preset.yaml is missing the 'states' array".to_string(),
        })?;

    let state_idx = find_state_index(states, &req.source_state_id).ok_or_else(|| {
        PresetError::NotFound(format!(
            "source state '{}' not found in Strategy '{strategy_id}'",
            req.source_state_id
        ))
    })?;

    let state_node = states
        .get_mut(state_idx)
        .ok_or_else(|| PresetError::Internal {
            code: "STRATEGY_STATE_INDEX".to_string(),
            message: "state index disappeared during transition patch".to_string(),
        })?;

    let op = req.op.as_str();

    let side_effects = if op == "create" {
        let new_target = req
            .new_target
            .as_deref()
            .ok_or_else(|| PresetError::Rejected {
                code: "strategy_transition_missing_new_target".to_string(),
                message: format!(
                    "new_target is required when creating a transition from state '{}'",
                    req.source_state_id
                ),
            })?;

        // For create, `next` may be absent; treat absent as null so a new
        // transition is inserted.
        if state_node.get("next").is_none() {
            state_node["next"] = serde_yaml::Value::Null;
        }
        let next = state_node
            .get_mut("next")
            .expect("next was just inserted if absent");
        apply_transition_create(next, req, new_target)?
    } else {
        // `op` defaults to "update"; any value other than "create" is treated as
        // an update to preserve backward compatibility with shipped callers.
        let old_target = req
            .old_target
            .as_deref()
            .ok_or_else(|| PresetError::Rejected {
                code: "strategy_transition_missing_old_target".to_string(),
                message: format!(
                    "old_target is required when updating a transition from state '{}'",
                    req.source_state_id
                ),
            })?;

        let next = state_node
            .get_mut("next")
            .ok_or_else(|| PresetError::Rejected {
                code: "strategy_transition_missing".to_string(),
                message: format!("state '{}' has no outgoing transition", req.source_state_id),
            })?;

        let (matched, side_effects) = apply_transition_patch(next, old_target, req);

        if !matched {
            return Err(PresetError::Rejected {
                code: "strategy_transition_not_found".to_string(),
                message: format!(
                    "no transition from '{}' to '{}' matches the request",
                    req.source_state_id, old_target
                ),
            });
        }
        side_effects
    };

    let (errors, warnings) = validate_preset_yaml(&bundle_dir, &yaml_value)?;
    if !errors.is_empty() {
        return Err(PresetError::strategy_validation_failed(
            &errors, &warnings,
        ));
    }

    let new_revision = current_revision.checked_add(1).ok_or_else(|| PresetError::Rejected { code: "strategy_revision_exhausted".into(), message: "strategy revision exhausted".into() })?;
    write_preset_yaml(&bundle_dir, &mut yaml_value, new_revision)?;

    Ok(CoreStrategyPatchResponse {
        new_revision: std::num::NonZeroU64::new(new_revision).unwrap_or(std::num::NonZeroU64::MIN),
        validation_summary: CoreStrategyPatchResponseValidationSummary { errors: vec![], warnings },
        side_effects,
    })
}
fn patch_prompt_template_inner(
    nexus_home: &std::path::Path,
    strategy_id: &str,
    state_id: &str,
    req: &StrategyPatchPromptTemplateRequest,
) -> Result<CoreStrategyPatchResponse, PresetError> {
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
    fn(&std::path::Path, &mut serde_yaml::Value, u64) -> Result<(), PresetError>;

fn patch_prompt_template_inner_with_writer(
    nexus_home: &std::path::Path,
    strategy_id: &str,
    state_id: &str,
    req: &StrategyPatchPromptTemplateRequest,
    write_yaml: PresetYamlWriter,
) -> Result<CoreStrategyPatchResponse, PresetError> {

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
    let new_revision = current_revision.checked_add(1).ok_or_else(|| PresetError::Rejected { code: "strategy_revision_exhausted".into(), message: "strategy revision exhausted".into() })?;

    // Validate the template path is safe before touching the filesystem.
    nexus_preset::loader::assert_template_file_safe(&req.template_ref).map_err(
        |reason| PresetError::Rejected {
            code: "strategy_template_path_unsafe".to_string(),
            message: reason,
        },
    )?;

    let canonical_template = confined_path(&bundle_dir, &bundle_dir.join(&req.template_ref))?;

    // Ensure parent directory exists inside the bundle.
    if let Some(parent) = canonical_template.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PresetError::Internal {
            code: "DIR_CREATE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    }

    let body = req.set.body.clone();

    // Stage the new template with a request-unique temp file, then rename it
    // into place before validating the manifest. If validation fails we roll
    // back to the previous file contents and never bump YAML revision.
    let backup = backup_existing_file(&canonical_template)?;

    let file_name = canonical_template
        .file_name()
        .ok_or_else(|| PresetError::Internal {
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

    // Validation can fail before returning diagnostics (e.g. malformed grammar).
    let (errors, warnings) = match validate_preset_yaml(&bundle_dir, &yaml_value) {
        Ok(summary) => summary,
        Err(error) => {
            rollback_template_write(&canonical_template, backup, &tmp_path);
            return Err(error);
        }
    };
    if !errors.is_empty() {
        rollback_template_write(&canonical_template, backup, &tmp_path);
        return Err(PresetError::strategy_validation_failed(
            &errors, &warnings,
        ));
    }

    // Persist the YAML revision only after the template file has been
    // committed. If YAML persistence fails, roll the template back so the
    // on-disk prompt bytes and the YAML revision can never diverge.
    if let Err(e) = write_yaml(&bundle_dir, &mut yaml_value, new_revision) {
        rollback_template_write(&canonical_template, backup, &tmp_path);
        return Err(e);
    }

    let mut side_effects: Vec<String> = Vec::new();
    side_effects.push(format!("wrote prompt template '{}'", req.template_ref));

    Ok(CoreStrategyPatchResponse {
        new_revision: std::num::NonZeroU64::new(new_revision).unwrap_or(std::num::NonZeroU64::MIN),
        validation_summary: CoreStrategyPatchResponseValidationSummary { errors: vec![], warnings },
        side_effects,
    })
}
/// Default maximum YAML file size for validation (1 MiB).
const VALIDATE_MAX_YAML_SIZE: usize = 1024 * 1024;

/// Default maximum YAML nesting depth for validation.
const VALIDATE_MAX_YAML_DEPTH: usize = 10;
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
          path: prompts/start.md
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
) -> Result<nexus_contracts::local::orchestration::preset::PresetManifest, PresetError> {
    let yaml_value: serde_yaml::Value = match serde_yaml::from_str(yaml) {
        Ok(v) => v,
        Err(e) => {
            return Err(PresetError::Internal {
                code: "YAML_PARSE_ERROR".into(),
                message: format!("YAML parse error: {e}"),
            });
        }
    };

    let depth = nexus_preset::yaml_value_depth(&yaml_value);
    if depth > VALIDATE_MAX_YAML_DEPTH {
        return Err(PresetError::Internal {
            code: "DEPTH_EXCEEDED".into(),
            message: format!("Nesting depth ({depth}) exceeds maximum ({VALIDATE_MAX_YAML_DEPTH})"),
        });
    }

    serde_yaml::from_value(yaml_value).map_err(|e| PresetError::Internal {
        code: "STRUCTURAL_ERROR".into(),
        message: format!("Structural validation error: {e}"),
    })
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

/// Locate a preset by ID and return its source + filesystem path.
fn locate_preset(
    nexus_home: &std::path::Path,
    preset_id: &str,
) -> Result<(String, Option<std::path::PathBuf>), PresetError> {
    validate_strategy_id(preset_id)?;
    if nexus_preset::list_embedded_presets().contains(&preset_id.to_string()) {
        return Ok(("embedded".to_string(), None));
    }

    let user_dir = user_preset_bundle_dir(raw_user_home(nexus_home)?, preset_id);
    if user_dir.join("preset.yaml").exists() {
        return Ok(("user".to_string(), Some(confined_path(&user_preset_base_dir(raw_user_home(nexus_home)?), &user_dir)?)));
    }

    let system_dir = system_preset_dir_for_id(nexus_home, preset_id);
    if system_dir.join("preset.yaml").exists() {
        return Ok(("system".to_string(), Some(confined_path(&nexus_home.join("presets/_system"), &system_dir)?)));
    }

    Err(PresetError::NotFound(format!(
        "Preset '{preset_id}' not found"
    )))
}

/// Load the raw YAML content for a preset.
fn load_preset_yaml(
    _nexus_home: &std::path::Path,
    preset_id: &str,
    source: &str,
    path: Option<&std::path::Path>,
) -> Result<String, PresetError> {
    match source {
        "embedded" => {
            let caps = BuiltinCapabilityCatalog;
            let loaded = nexus_preset::load_embedded_preset(preset_id, &caps)
                .map_err(|e| PresetError::Internal {
                    code: "PRESET_LOAD_ERROR".to_string(),
                    message: e.to_string(),
                })?;
            serde_yaml::to_string(&loaded.manifest).map_err(|e| PresetError::Internal {
                code: "YAML_SERIALIZE_ERROR".to_string(),
                message: e.to_string(),
            })
        }
        "system" | "user" => {
            let dir = path.ok_or_else(|| PresetError::Internal {
                code: "PRESET_PATH_MISSING".to_string(),
                message: format!("{source} preset '{preset_id}' has no path"),
            })?;
            let yaml_path = confined_path(dir, &dir.join("preset.yaml"))?;
            std::fs::read_to_string(&yaml_path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    PresetError::NotFound(format!("Preset '{preset_id}' not found"))
                } else {
                    PresetError::Internal {
                        code: "FILE_READ_ERROR".to_string(),
                        message: e.to_string(),
                    }
                }
            })
        }
        _ => Err(PresetError::Internal {
            code: "UNKNOWN_PRESET_SOURCE".to_string(),
            message: format!("unknown preset source '{source}'"),
        }),
    }
}

fn validate_preset_file(req: &ValidatePresetRequest) -> Result<ValidatePresetResponse, PresetError> {
    let file_path = std::path::Path::new(&req.path);

    if !file_path.exists() {
        return Err(PresetError::NotFound(format!(
            "File not found: {}",
            file_path.display()
        )));
    }

    // Check file size via metadata BEFORE reading
    let metadata = std::fs::metadata(file_path).map_err(|e| PresetError::Internal {
        code: "METADATA_ERROR".into(),
        message: e.to_string(),
    })?;
    if metadata.len() > VALIDATE_MAX_YAML_SIZE as u64 {
        return Ok(invalid_validation(&[format!(
            "Preset YAML exceeds maximum size ({} bytes, limit is {} bytes)",
            metadata.len(),
            VALIDATE_MAX_YAML_SIZE
        )]));
    }

    let yaml = std::fs::read_to_string(file_path).map_err(|e| PresetError::Internal {
        code: "FILE_READ_ERROR".into(),
        message: e.to_string(),
    })?;

    // Defense-in-depth size check
    if yaml.len() > VALIDATE_MAX_YAML_SIZE {
        return Ok(invalid_validation(&[format!(
            "Preset YAML exceeds maximum size ({} bytes, limit is {} bytes)",
            yaml.len(),
            VALIDATE_MAX_YAML_SIZE
        )]));
    }

    // Parse + depth check
    let manifest = parse_and_check_manifest(&yaml)?;

    // C2: Run loader-equivalent structural validation so the daemon endpoint
    //     rejects the same defects the runtime loader would reject.
    let caps = BuiltinCapabilityCatalog;
    let structural_problems =
        nexus_preset::loader_validate_manifest_compat(&manifest, &caps);
    if !structural_problems.is_empty() {
        let errors: Vec<String> = structural_problems
            .iter()
            .map(|p| format!("{}: {}", p.path, p.error))
            .collect();
        return Ok(ValidatePresetResponse {
            valid: false,
            id: Some(manifest.preset.id.clone()),
            version: Some(i64::from(manifest.preset.version)),
            state_count: Some(i64::try_from(manifest.states.len()).expect("bounded YAML state count")),
            errors,
            warnings: Vec::new(),
        });
    }

    // C3: Shared path-safety check (same `assert_template_file_safe` the loader uses).
    let path_result = nexus_preset::validate_path_safety(&manifest);

    // A5: Run shared semantic validation (the same surface used by the loader).
    let sem_result = nexus_preset::validate_preset_semantic(&manifest, &caps);

    // A3: If the path points into a bundle directory, also run asset checks.
    let asset_result = infer_bundle_root(file_path).map_or_else(
        nexus_preset::ValidationResult::default,
        |bundle_root| {
            nexus_preset::validate_assets_in_bundle(&manifest, &bundle_root)
        },
    );

    // Combine diagnostics from path safety + semantic + asset checks
    let mut errors: Vec<String> = Vec::new();
    for d in path_result
        .diagnostics
        .iter()
        .chain(sem_result.diagnostics.iter())
        .chain(asset_result.diagnostics.iter())
        .filter(|d| d.severity == nexus_preset::DiagnosticSeverity::Error)
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
        .filter(|d| d.severity == nexus_preset::DiagnosticSeverity::Warning)
        .map(|d| format!("{}: {}", d.path, d.message))
        .collect();

    let valid = errors.is_empty();
    Ok(ValidatePresetResponse {
        valid,
        id: Some(manifest.preset.id.clone()),
        version: Some(i64::from(manifest.preset.version)),
        state_count: Some(i64::try_from(manifest.states.len()).expect("bounded YAML state count")),
        errors,
        warnings,
    })
}

fn invalid_validation(errors: &[String]) -> ValidatePresetResponse {
    ValidatePresetResponse { valid: false, id: None, version: None, state_count: None, errors: errors.to_vec(), warnings: vec![] }
}

fn scaffold_user_preset(nexus_home: &Path, id: &str) -> Result<ScaffoldPresetResponse, PresetError> {
    let bundle = confined_path(&user_preset_base_dir(raw_user_home(nexus_home)?), &user_preset_bundle_dir(raw_user_home(nexus_home)?, id))?;
    if bundle.exists() {
        return Err(PresetError::Conflict(format!("Preset '{id}' already exists at {}", bundle.display())));
    }
    // Encode the identifier as a YAML scalar rather than interpolating YAML syntax.
    let quoted = serde_json::to_string(id).map_err(|e| preset_io("YAML_SERIALIZE_ERROR", e))?;
    let yaml = PRESET_INIT_TEMPLATE.replace("{{name}}", &quoted);
    parse_and_check_manifest(&yaml)?;
    std::fs::create_dir(&bundle).map_err(|e| preset_io("DIR_CREATE_ERROR", e))?;
    let prompts = bundle.join("prompts");
    std::fs::create_dir(&prompts).map_err(|e| preset_io("DIR_CREATE_ERROR", e))?;
    std::fs::write(prompts.join("start.md"), PROMPT_INIT_CONTENT).map_err(|e| preset_io("FILE_WRITE_ERROR", e))?;
    let tmp = bundle.join(format!("preset.yaml.tmp.{}", uuid::Uuid::new_v4()));
    atomic_write_with_dir_fsync(&bundle.join("preset.yaml"), &tmp, yaml.as_bytes())?;
    Ok(ScaffoldPresetResponse { id: id.to_string(), path: bundle.display().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_write_failure_rolls_back_prompt_and_keeps_revision() {
        fn fail_write(_: &Path, _: &mut serde_yaml::Value, _: u64) -> Result<(), PresetError> {
            Err(preset_io("INJECTED_YAML_WRITE_ERROR", "injected persistence failure"))
        }
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join(".nexus42");
        let bundle = home.join("presets/rollback");
        std::fs::create_dir_all(bundle.join("prompts")).unwrap();
        let yaml = r#"revision: 1
preset:
  id: rollback
  version: 1
  kind: creator
  description: Prompt rollback regression
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    context_update:
      op: { kind: append, body: "" }
      template_file: prompts/original.md
    next: end
  - id: end
    terminal: true
"#;
        let yaml_path = bundle.join("preset.yaml");
        let prompt_path = bundle.join("prompts/original.md");
        std::fs::write(&yaml_path, yaml).unwrap();
        std::fs::write(&prompt_path, "Original").unwrap();
        let request = serde_json::from_value(serde_json::json!({
            "strategy_id": "rollback", "state_id": "start", "base_revision": 1,
            "template_ref": "prompts/original.md", "set": {"body": "Replacement"}
        })).unwrap();
        let _lock = acquire_preset_lock(&home, "rollback").unwrap();
        let error = patch_prompt_template_inner_with_writer(&home, "rollback", "start", &request, fail_write).unwrap_err();
        assert!(matches!(&error, PresetError::Internal { code, .. } if code == "INJECTED_YAML_WRITE_ERROR"));
        assert_eq!(std::fs::read_to_string(&prompt_path).unwrap(), "Original");
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), yaml);
    }

    #[test]
    fn conditional_transition_rejects_duplicate_but_keeps_distinct_condition() {
        let mut next = serde_yaml::Mapping::new();
        let request: StrategyPatchTransitionRequest = serde_json::from_value(serde_json::json!({
            "strategy_id": "strategy", "source_state_id": "start", "base_revision": 1,
            "condition": "_context.ready", "op": "update"
        })).unwrap();
        append_conditional_rule(&mut next, &request, "end").unwrap();
        let error = append_conditional_rule(&mut next, &request, "end").unwrap_err();
        assert!(matches!(&error, PresetError::Rejected { code, .. } if code == "strategy_transition_duplicate"));
        let different = StrategyPatchTransitionRequest {
            condition: Some("_context.alternate".into()), ..request
        };
        append_conditional_rule(&mut next, &different, "end").unwrap();
        let rules = next["rules"].as_sequence().unwrap();
        assert_eq!(rules[0]["when"].as_str(), Some("_context.ready"));
        assert_eq!(rules[1]["when"].as_str(), Some("_context.alternate"));
    }
}
/// Trigger-lane classification (AR-21).
///
/// `cron` is derived from the shared works-cron role membership
/// ([`cron_role_preset_ids`] — the brainstorm / write / review role presets
/// per `RolesSchedule`; same source as
/// `schedule::cron_supervisor::role_preset`), never a hand-maintained
/// per-preset list (W-001/F-004).
///
/// `session` is honest per resolvability class (W-003/F-002): the
/// session-start API (`POST /v1/daemon/orchestration/sessions`) loads
/// embedded presets only (`load_embedded_preset`), so a **user** preset
/// reports `session: false` — the lane claim must not overstate what the
/// runtime can serve. System (`_system.`) and embedded presets report
/// `session: true`.
///
/// `wall_clock` / `direct` are platform facts — the daemon schedule path
/// resolves any resolvable preset id (`resolve_preset`), so every resolvable
/// preset can fire on the wall-clock poller or via a direct run with an
/// explicit payload.
fn profile_lanes(preset_id: &str, session: bool) -> PresetProfileLanes {
    PresetProfileLanes {
        cron: cron_role_preset_ids().contains(&preset_id),
        wall_clock: true,
        session,
        direct: true,
    }
}

/// Is `preset_id` a user preset (3-tier resolvability class, AR-22)?
///
/// `_system.` qualified ids are system presets; otherwise a user bundle at
/// `<nexus_home>/presets/<id>/preset.yaml` marks the user class. Used for
/// the `session` lane honesty check (W-003/F-002).
fn is_user_preset(preset_id: &str, nexus_home: &std::path::Path) -> bool {
    !preset_id.starts_with("_system.")
        && nexus_home
            .join("presets")
            .join(preset_id)
            .join("preset.yaml")
            .is_file()
}

/// Map one manifest state to its profile shape.
fn profile_state(state: &StateDefinition) -> PresetProfileState {
    PresetProfileState {
        id: state.id.clone(),
        description: state.description.clone(),
        enter: state.enter.iter().map(profile_enter_action).collect(),
        exit_when: state.exit_when.as_ref().map(profile_exit_when),
        next: state.next.as_ref().map(profile_next),
        terminal: state.terminal,
    }
}

/// Map one enter action to its profile shape.
fn profile_enter_action(action: &EnterAction) -> PresetProfileEnterAction {
    match action {
        EnterAction::Capability { name, .. } => PresetProfileEnterAction {
            kind: "capability".to_string(),
            name: name.clone(),
        },
        EnterAction::InnerGraph { name } => PresetProfileEnterAction {
            kind: "inner_graph".to_string(),
            name: name.clone(),
        },
        EnterAction::HostTool { tool_name, .. } => PresetProfileEnterAction {
            kind: "host_tool".to_string(),
            name: tool_name.clone(),
        },
    }
}

/// Map one exit condition to its profile shape.
fn profile_exit_when(exit_when: &ExitWhen) -> PresetProfileExitWhen {
    match exit_when {
        ExitWhen::LlmJudge {
            template_file,
            judge_capability,
            min_interval,
        } => PresetProfileExitWhen {
            kind: "llm_judge".to_string(),
            template_file: template_file.clone(),
            judge_capability: judge_capability.clone(),
            min_interval: min_interval.clone(),
            duration: None,
        },
        ExitWhen::Rule => PresetProfileExitWhen {
            kind: "rule".to_string(),
            template_file: None,
            judge_capability: None,
            min_interval: None,
            duration: None,
        },
        ExitWhen::GraphComplete => PresetProfileExitWhen {
            kind: "graph_complete".to_string(),
            template_file: None,
            judge_capability: None,
            min_interval: None,
            duration: None,
        },
        ExitWhen::Manual => PresetProfileExitWhen {
            kind: "manual".to_string(),
            template_file: None,
            judge_capability: None,
            min_interval: None,
            duration: None,
        },
        ExitWhen::Timer { duration } => PresetProfileExitWhen {
            kind: "timer".to_string(),
            template_file: None,
            judge_capability: None,
            min_interval: None,
            duration: duration.clone(),
        },
    }
}

/// Map one next-transition form to its profile shape.
fn profile_next(next: &NextTarget) -> PresetProfileNext {
    match next {
        NextTarget::Linear(target) => PresetProfileNext {
            kind: "linear".to_string(),
            target: Some(target.clone()),
            go: None,
            nogo: None,
            labeled: Vec::new(),
            rules: Vec::new(),
            branches: Vec::new(),
            default: None,
        },
        NextTarget::GoNogo(go_nogo) => PresetProfileNext {
            kind: "goNogo".to_string(),
            target: None,
            go: Some(go_nogo.go.clone()),
            nogo: Some(go_nogo.nogo.clone()),
            labeled: Vec::new(),
            rules: Vec::new(),
            branches: Vec::new(),
            default: None,
        },
        NextTarget::Labeled(edges) => PresetProfileNext {
            kind: "labeled".to_string(),
            target: None,
            go: None,
            nogo: None,
            labeled: edges
                .iter()
                .map(|e| PresetProfileLabeledNext {
                    label: e.label.clone(),
                    target: e.target.clone(),
                })
                .collect(),
            rules: Vec::new(),
            branches: Vec::new(),
            default: None,
        },
        NextTarget::Conditional(cond) => PresetProfileNext {
            kind: "conditional".to_string(),
            target: None,
            go: None,
            nogo: None,
            labeled: Vec::new(),
            rules: cond
                .rules
                .iter()
                .map(|r| PresetProfileConditionalRule {
                    when: r.when.clone(),
                    target: r.target.clone(),
                })
                .collect(),
            branches: Vec::new(),
            default: Some(cond.default.clone()),
        },
        NextTarget::Branches(branches) => PresetProfileNext {
            kind: "branches".to_string(),
            target: None,
            go: None,
            nogo: None,
            labeled: Vec::new(),
            rules: Vec::new(),
            branches: branches
                .branches
                .iter()
                .map(|r| PresetProfileConditionalRule {
                    when: r.when.clone(),
                    target: r.target.clone(),
                })
                .collect(),
            default: Some(branches.default.clone()),
        },
    }
}

/// Map one role definition to its profile shape.
fn profile_role(role: &PresetRoleDefinition) -> PresetProfileRole {
    PresetProfileRole {
        id: role.id.clone(),
        description: role.description.clone(),
        system_prompt_file: role.system_prompt_file.clone(),
        recommended_skills: role.recommended_skills.clone(),
    }
}

/// Map one declared signal binding to its profile shape.
fn profile_signal(signal: &SignalBinding) -> PresetProfileSignal {
    let action = match signal.on_receive.action {
        SignalActionKind::Pause => "pause",
        SignalActionKind::ForceTransition => "force_transition",
    };
    PresetProfileSignal {
        name: signal.name.clone(),
        action: action.to_string(),
        target: signal.on_receive.target.clone(),
    }
}
