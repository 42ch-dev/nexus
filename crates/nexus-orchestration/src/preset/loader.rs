//! Preset loader + validation.
//!
//! Parses `preset.yaml` → `PresetManifest` → validates per §7.6 → produces
//! a `LoadedPreset` with outer/inner `graph-flow::Graph` instances.
//!
//! Design: `orchestration-engine.md` §8.1.

use crate::capability::CapabilityRegistry;
use crate::preset::manifest::{
    ContextUpdateOp, ExitWhen, InitialAction, InnerGraph, NextTarget, PresetManifest,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// LoadedPreset
// ---------------------------------------------------------------------------

/// A fully validated preset ready for execution.
///
/// Design: `orchestration-engine.md` §8.1.
#[derive(Clone)]
pub struct LoadedPreset {
    /// Preset identifier.
    pub id: String,
    /// Preset schema version.
    pub version: u32,
    /// The outer state-machine graph (without engine wiring).
    pub outer_graph: Arc<graph_flow::Graph>,
    /// Named inner graphs (keyed by `inner_graphs.<name>`).
    pub inner_graphs: HashMap<String, Arc<graph_flow::Graph>>,
    /// Signal bindings declared in the manifest.
    pub signals: Vec<crate::preset::manifest::SignalBinding>,
    /// blake3 hash of the source YAML (identity across restarts).
    pub source_hash: [u8; 32],
    /// Output bindings per inner graph: name → binding string.
    pub output_bindings: HashMap<String, String>,
    /// The parsed manifest (retained for re-wiring outer graph with engine).
    pub manifest: PresetManifest,
    /// Initial action for schedule creation (from `preset.initial_action`).
    pub initial_action: Option<crate::preset::manifest::InitialAction>,
    /// Per-state context update hooks (keyed by state ID).
    pub context_update_hooks: HashMap<String, crate::preset::manifest::ContextUpdateHook>,
    /// Role definitions for multi-agent presets (WS-E T6).
    /// Empty = single-agent mode (backward compatible).
    pub roles: Vec<crate::preset::manifest::PresetRoleDefinition>,
}

impl std::fmt::Debug for LoadedPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPreset")
            .field("id", &self.id)
            .field("version", &self.version)
            .field("outer_graph_id", &self.outer_graph.id)
            .field(
                "inner_graphs_keys",
                &self.inner_graphs.keys().collect::<Vec<_>>(),
            )
            .field("signals_len", &self.signals.len())
            .field("source_hash", &format!("{:02x?}", &self.source_hash[..4]))
            .field("output_bindings", &self.output_bindings)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// PresetLoadError
// ---------------------------------------------------------------------------

/// Default maximum YAML file size for user-supplied presets (1 MiB).
pub const DEFAULT_MAX_YAML_SIZE: usize = 1024 * 1024;

/// Default maximum YAML nesting depth for user-supplied presets.
pub const DEFAULT_MAX_YAML_DEPTH: usize = 10;

/// Structured error listing every problem found during preset loading.
#[derive(Error, Debug)]
pub enum PresetLoadError {
    /// YAML parse error.
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),
    /// One or more validation problems found.
    #[error("preset validation failed ({len} problem(s))")]
    Validation {
        /// Structured list of problems.
        problems: Vec<ValidationProblem>,
        /// Number of problems (display only).
        len: usize,
    },
    /// A preset hook used an invalid operation kind (e.g. replace).
    #[error("invalid preset hook operation: {0}")]
    InvalidPresetHookOp(String),
    /// No embedded preset with the given ID was found.
    #[error("preset not found: {preset_id}")]
    NotFound {
        /// The preset ID that was not found.
        preset_id: String,
    },
    /// YAML file exceeds the maximum allowed size.
    #[error(
        "preset YAML exceeds maximum size ({actual} bytes, limit is {limit} bytes). \
            Simplify the preset or split into smaller presets."
    )]
    YamlSizeExceeded {
        /// Actual size in bytes.
        actual: usize,
        /// Maximum allowed size in bytes.
        limit: usize,
    },
    /// YAML nesting depth exceeds the maximum allowed depth.
    #[error(
        "preset YAML nesting depth ({actual}) exceeds maximum ({limit}). \
            Flatten deeply nested structures in your preset."
    )]
    YamlDepthExceeded {
        /// Actual nesting depth.
        actual: usize,
        /// Maximum allowed depth.
        limit: usize,
    },
}

impl PresetLoadError {
    /// Borrow the list of validation problems (if this is a validation error).
    #[must_use]
    pub fn problems(&self) -> &[ValidationProblem] {
        match self {
            Self::Validation { problems, .. } => problems,
            _ => &[],
        }
    }
}

/// A single validation problem found during preset loading.
#[derive(Debug, Clone)]
pub struct ValidationProblem {
    /// Dot-path to the offending field (e.g. `"states[1].enter[0].name"`).
    pub path: String,
    /// Human-readable error description.
    pub error: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load a preset from a YAML string.
///
/// Validates all §7.6 rules. Does NOT validate template file paths against a
/// filesystem root (use [`load_preset`] for that).
///
/// `source_hash` is blake3 over the YAML string.
///
/// # Errors
/// Returns [`PresetLoadError`] if YAML parsing, validation, or graph construction fails.
pub fn load_preset_from_str(
    yaml: &str,
    caps: &CapabilityRegistry,
) -> Result<LoadedPreset, PresetLoadError> {
    load_preset_from_str_with_limits(yaml, caps, DEFAULT_MAX_YAML_SIZE, DEFAULT_MAX_YAML_DEPTH)
}

/// Load a preset from a YAML string with explicit limits.
///
/// Like [`load_preset_from_str`] but allows overriding the default size and
/// depth limits. Embedded/bundled presets may call this with higher limits.
///
/// # Errors
/// Returns [`PresetLoadError`] if YAML parsing, limit checks, or validation fails.
pub fn load_preset_from_str_with_limits(
    yaml: &str,
    caps: &CapabilityRegistry,
    max_size: usize,
    max_depth: usize,
) -> Result<LoadedPreset, PresetLoadError> {
    // 0a. Size check.
    if yaml.len() > max_size {
        return Err(PresetLoadError::YamlSizeExceeded {
            actual: yaml.len(),
            limit: max_size,
        });
    }

    // 0b. Depth check — parse to Value, measure depth, then deserialize from
    //     the same Value tree (single parse avoids double stack-overflow surface).
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    let actual_depth = yaml_value_depth(&yaml_value);
    if actual_depth > max_depth {
        return Err(PresetLoadError::YamlDepthExceeded {
            actual: actual_depth,
            limit: max_depth,
        });
    }

    // 0c. Unknown top-level key check (R-V137P0-01).
    //     serde silently ignores unknown keys; warn so mis-placed sections
    //     (e.g. `gates:` at root instead of under `preset:`) are surfaced.
    warn_unknown_top_level_keys(&yaml_value);

    // 1. Deserialize from the already-parsed Value (avoids double-parse that widens
    //    the stack-overflow attack surface — QC3 W-002).
    let manifest: PresetManifest =
        serde_yaml::from_value(yaml_value).map_err(PresetLoadError::from)?;

    // 2. Structural validation (legacy §7.6 rules).
    let problems = validate_manifest(&manifest, caps);
    if !problems.is_empty() {
        return Err(PresetLoadError::Validation {
            len: problems.len(),
            problems,
        });
    }

    // 2b. Semantic validation (V1.32 P1 facade — reachability, terminal consistency,
    //     id match, orphan inner graphs). Only certain error categories are fatal for
    //     the loader; capability arg drift is reported as warnings because existing
    //     embedded presets may not perfectly match schema metadata.
    let sem = super::validation::validate_preset_semantic(&manifest, caps);
    if sem.errors().any(|d| {
        matches!(
            d.category,
            super::validation::DiagnosticCategory::Reachability
                | super::validation::DiagnosticCategory::TerminalConsistency
                | super::validation::DiagnosticCategory::IdMismatch
                | super::validation::DiagnosticCategory::Structural
                | super::validation::DiagnosticCategory::MissingAsset
                | super::validation::DiagnosticCategory::PathSafety
                | super::validation::DiagnosticCategory::MissingCapability
        )
    }) {
        let all_problems: Vec<ValidationProblem> = sem.to_problems();
        return Err(PresetLoadError::Validation {
            len: all_problems.len(),
            problems: all_problems,
        });
    }
    // Log all warnings + capability arg drift errors as warnings.
    for d in sem.diagnostics.iter().filter(|d| {
        d.severity == super::validation::DiagnosticSeverity::Warning
            || d.category == super::validation::DiagnosticCategory::CapabilityArgDrift
            || d.category == super::validation::DiagnosticCategory::SchemaCheckSkipped
    }) {
        tracing::warn!(path = %d.path, message = %d.message, category = ?d.category, "preset validation warning");
    }

    // 3. Build outer graph per §8.2 mapping table.
    let outer_graph = build_outer_graph(&manifest);

    // 4. Build inner graphs per §8.2 mapping table.
    let inner_graphs = build_inner_graphs(&manifest);

    // 5. Extract output bindings from manifest.
    let output_bindings = extract_output_bindings(&manifest);

    // 6. Compute source hash.
    let hash = blake3::hash(yaml.as_bytes());
    let mut source_hash = [0u8; 32];
    source_hash.copy_from_slice(hash.as_bytes());

    Ok(LoadedPreset {
        id: manifest.preset.id.clone(),
        version: manifest.preset.version,
        outer_graph: Arc::new(outer_graph),
        inner_graphs,
        signals: manifest.signals.clone(),
        source_hash,
        output_bindings,
        initial_action: manifest.preset.initial_action.clone(),
        context_update_hooks: manifest
            .states
            .iter()
            .filter_map(|s| {
                s.context_update
                    .as_ref()
                    .map(|hook| (s.id.clone(), hook.clone()))
            })
            .collect(),
        roles: manifest.roles.clone(),
        manifest,
    })
}

/// Load a preset from a bundle directory on disk.
///
/// Reads `preset.yaml` from the bundle root and delegates to
/// [`load_preset_from_str`]. Adds filesystem-level sandbox validation
/// that `template_file` paths resolve within the bundle root.
///
/// # Errors
/// Returns [`PresetLoadError`] if file reading, parsing, or validation fails.
pub fn load_preset(
    bundle_root: &Path,
    caps: &CapabilityRegistry,
) -> Result<LoadedPreset, PresetLoadError> {
    let preset_yaml_path = bundle_root.join("preset.yaml");
    let yaml =
        std::fs::read_to_string(&preset_yaml_path).map_err(|e| PresetLoadError::Validation {
            len: 1,
            problems: vec![ValidationProblem {
                path: preset_yaml_path.to_string_lossy().to_string(),
                error: format!("failed to read preset.yaml: {e}"),
            }],
        })?;

    // WS-B T3: parse first, then sandbox-validate template_file paths
    let manifest: PresetManifest = serde_yaml::from_str(&yaml)?;

    // A3 parity: use the same shared validation surface as the daemon endpoint.
    // validate_path_safety() checks structural safety (no '..' traversal, no absolute paths).
    // validate_assets_in_bundle() checks file existence + symlink escape + id vs directory.
    let a3_path_result = super::validation::validate_path_safety(&manifest);
    let a3_asset_result = super::validation::validate_assets_in_bundle(&manifest, bundle_root);

    // Collect all A3 error-severity diagnostics as validation problems.
    let mut a3_problems: Vec<ValidationProblem> = Vec::new();
    for d in a3_path_result.errors().chain(a3_asset_result.errors()) {
        a3_problems.push(ValidationProblem {
            path: d.path.clone(),
            error: d.message.clone(),
        });
    }
    if !a3_problems.is_empty() {
        return Err(PresetLoadError::Validation {
            len: a3_problems.len(),
            problems: a3_problems,
        });
    }

    // Log A3 warnings (informational; do not block loading).
    for d in a3_path_result.warnings().chain(a3_asset_result.warnings()) {
        tracing::warn!(path = %d.path, message = %d.message, category = ?d.category, "A3 loader warning");
    }

    load_preset_from_str(&yaml, caps)
}

// ---------------------------------------------------------------------------
// Template path safety check (WS-B: modeled after assert_creator_id_safe)
// ---------------------------------------------------------------------------

/// Assert that a `template_file` value does not contain path-traversal patterns.
///
/// Returns `Ok(())` for safe relative paths, `Err(String)` with a descriptive
/// message for dangerous patterns.
///
/// Rejected patterns (consistent with `nexus-home-layout::assert_creator_id_safe`):
/// - `..` (directory traversal)
/// - `/` prefix (absolute path)
/// - null bytes
/// - control characters
///
/// # Errors
/// Returns [`LoaderError`] if the template file path is invalid (contains `..`, is absolute, or parent traversal).
pub fn assert_template_file_safe(path: &str) -> Result<(), String> {
    if path.starts_with('/') {
        return Err(format!(
            "template_file must be a relative path: {path:?} (absolute paths are not allowed)"
        ));
    }
    if path.contains('\\') {
        return Err(format!(
            "template_file contains backslash: {path:?} (backslash separators are not allowed)"
        ));
    }
    if path.contains("..") {
        return Err(format!(
            "template_file contains '..': {path:?} (directory traversal is not allowed)"
        ));
    }
    if path.contains('\0') {
        return Err(format!("template_file contains null bytes: {path:?}"));
    }
    if path.chars().any(char::is_control) {
        return Err(format!(
            "template_file contains control characters: {path:?}"
        ));
    }
    Ok(())
}

/// Collect all `template_file` paths from a manifest as `(dot_path, &str)` pairs.
///
/// Shared by [`validate_manifest`] and [`validate_template_files_in_sandbox`]
/// to avoid duplicating the manifest-walking logic.
fn collect_template_file_entries(manifest: &PresetManifest) -> Vec<(String, &str)> {
    let mut entries: Vec<(String, &str)> = Vec::new();

    // Outer state exit_when template_file
    for (i, state) in manifest.states.iter().enumerate() {
        if let Some(ExitWhen::LlmJudge {
            template_file: Some(ref tf),
            ..
        }) = state.exit_when
        {
            entries.push((format!("states[{i}].exit_when.template_file"), tf));
        }
        // context_update template_file
        if let Some(ref hook) = state.context_update {
            entries.push((
                format!("states[{i}].context_update.template_file"),
                &hook.template_file,
            ));
        }
    }

    // Inner graph node template_file
    if let Some(ref inner_graphs) = manifest.inner_graphs {
        for (name, ig) in inner_graphs {
            for (k, node) in ig.nodes.iter().enumerate() {
                if let Some(ref tf) = node.template_file {
                    entries.push((format!("inner_graphs.{name}.nodes[{k}].template_file"), tf));
                }
            }
        }
    }

    // initial_action template_file
    if let Some(InitialAction::SeedExpansion {
        template_file: Some(ref tf),
        ..
    }) = manifest.preset.initial_action
    {
        entries.push(("preset.initial_action.template_file".to_string(), tf));
    }

    entries
}

// ---------------------------------------------------------------------------
// Validation (§7.6)
// ---------------------------------------------------------------------------

/// Public wrapper around private `validate_manifest`.
///
/// For callers that have already parsed a manifest and need
/// loader-equivalent structural checks without loading the full preset
/// (e.g. the daemon validate endpoint).
///
/// This is the same set of checks that `load_preset_from_str_with_limits` runs
/// at step 2. It does NOT include semantic checks (use
/// `validation::validate_preset_semantic` for those).
#[must_use]
pub fn loader_validate_manifest_compat(
    manifest: &PresetManifest,
    caps: &CapabilityRegistry,
) -> Vec<ValidationProblem> {
    validate_manifest(manifest, caps)
}

/// Run all §7.6 validation rules against a parsed manifest.
///
/// Returns a list of problems (empty = valid).
///
/// # Errors
/// This function does not return errors, it returns validation problems.
#[allow(clippy::too_many_lines)]
fn validate_manifest(
    manifest: &PresetManifest,
    caps: &CapabilityRegistry,
) -> Vec<ValidationProblem> {
    let mut problems = Vec::new();

    let state_ids: HashSet<&str> = manifest.states.iter().map(|s| s.id.as_str()).collect();

    // --- Field type checks (serde already handles most, but we add semantic checks) ---

    // Validate requires_capabilities
    for (i, req_cap) in manifest.preset.requires_capabilities.iter().enumerate() {
        if caps.get(req_cap).is_none() {
            problems.push(ValidationProblem {
                path: format!("preset.requires_capabilities[{i}]"),
                error: format!("required capability not found in registry: '{req_cap}'"),
            });
        }
    }

    // initial must exist
    if !state_ids.contains(manifest.preset.initial.as_str()) {
        problems.push(ValidationProblem {
            path: "preset.initial".into(),
            error: format!("unknown state: '{}'", manifest.preset.initial),
        });
    }

    // terminal must exist
    if !state_ids.contains(manifest.preset.terminal.as_str()) {
        problems.push(ValidationProblem {
            path: "preset.terminal".into(),
            error: format!("unknown state: '{}'", manifest.preset.terminal),
        });
    }

    // Validate each state
    for (i, state) in manifest.states.iter().enumerate() {
        let state_path = format!("states[{i}]");

        // Check next state reference
        if let Some(ref next) = state.next {
            match next {
                NextTarget::Linear(target_id) => {
                    if !state_ids.contains(target_id.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.next"),
                            error: format!("unknown state: '{target_id}'"),
                        });
                    }
                }
                NextTarget::GoNogo(go_nogo) => {
                    // V1.42 P2: GoNogo is only valid on llm_judge states.
                    if !matches!(state.exit_when, Some(ExitWhen::LlmJudge { .. })) {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.next"),
                            error: "go/nogo conditional next is only valid on llm_judge states"
                                .to_string(),
                        });
                    }
                    if !state_ids.contains(go_nogo.go.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.next.go"),
                            error: format!("unknown state: '{}'", go_nogo.go),
                        });
                    }
                    if !state_ids.contains(go_nogo.nogo.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.next.nogo"),
                            error: format!("unknown state: '{}'", go_nogo.nogo),
                        });
                    }
                }
                NextTarget::Labeled(labeled_edges) => {
                    // V1.52 T-B P0: Labeled edges are only valid on llm_judge states.
                    if !matches!(state.exit_when, Some(ExitWhen::LlmJudge { .. })) {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.next"),
                            error: "labeled conditional next is only valid on llm_judge states"
                                .to_string(),
                        });
                    }
                    for (k, edge) in labeled_edges.iter().enumerate() {
                        if !state_ids.contains(edge.target.as_str()) {
                            problems.push(ValidationProblem {
                                path: format!("{state_path}.next[{k}].target"),
                                error: format!("unknown state: '{}'", edge.target),
                            });
                        }
                    }
                }
                NextTarget::Conditional(next_cond) => {
                    // V1.56 P2: Conditional next is now accepted on any state kind.
                    // Legacy `kind: conditional` form (rules field).
                    for (k, rule) in next_cond.rules.iter().enumerate() {
                        if !state_ids.contains(rule.target.as_str()) {
                            problems.push(ValidationProblem {
                                path: format!("{state_path}.next.rules[{k}].target"),
                                error: format!("unknown state: '{}'", rule.target),
                            });
                        }
                    }
                    if !state_ids.contains(next_cond.default.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.next.default"),
                            error: format!("unknown default state: '{}'", next_cond.default),
                        });
                    }
                }
                NextTarget::Branches(branches) => {
                    // V1.56 P2: Form B — expression/rule-based multi-branch.
                    for (k, rule) in branches.branches.iter().enumerate() {
                        if !state_ids.contains(rule.target.as_str()) {
                            problems.push(ValidationProblem {
                                path: format!("{state_path}.next.branches[{k}].target"),
                                error: format!("unknown state: '{}'", rule.target),
                            });
                        }
                    }
                    if !state_ids.contains(branches.default.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.next.default"),
                            error: format!("unknown default state: '{}'", branches.default),
                        });
                    }
                }
            }
        }

        // Check that the terminal state has no next
        if state.terminal && state.next.is_some() {
            problems.push(ValidationProblem {
                path: format!("{state_path}.terminal"),
                error: "terminal state must not have a 'next' field".to_string(),
            });
        }

        // Check enter actions
        for (j, enter) in state.enter.iter().enumerate() {
            let enter_path = format!("{state_path}.enter[{j}]");
            match enter {
                crate::preset::manifest::EnterAction::Capability { name, .. } => {
                    if caps.get(name).is_none() {
                        problems.push(ValidationProblem {
                            path: format!("{enter_path}.name"),
                            error: format!("unknown capability: '{name}'"),
                        });
                    }
                }
                crate::preset::manifest::EnterAction::InnerGraph { name } => {
                    // Check inner_graph exists
                    let has_inner = manifest
                        .inner_graphs
                        .as_ref()
                        .is_some_and(|ig| ig.contains_key(name));
                    if !has_inner {
                        problems.push(ValidationProblem {
                            path: format!("{enter_path}.name"),
                            error: format!("unknown inner_graph: '{name}'"),
                        });
                    }
                }
                crate::preset::manifest::EnterAction::HostTool { .. } => {
                    // HostTool actions are dispatched through the daemon's
                    // unified registry, not the capability registry. No
                    // static validation needed.
                }
            }
        }

        // Check exit_when judge_capability
        if let Some(ExitWhen::LlmJudge {
            judge_capability: Some(ref cap_name),
            ..
        }) = state.exit_when
        {
            if caps.get(cap_name).is_none() {
                problems.push(ValidationProblem {
                    path: format!("{state_path}.exit_when.judge_capability"),
                    error: format!("unknown capability: '{cap_name}'"),
                });
            }
        }

        // Validate context_update hook (WS7 §7)
        if let Some(ref hook) = state.context_update {
            match &hook.op {
                ContextUpdateOp::Append { .. } | ContextUpdateOp::StructMerge { .. } => {}
                ContextUpdateOp::LlmSummarize { capability } => {
                    // Validate that the referenced capability exists
                    if caps.get(capability).is_none() {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.context_update.op.capability"),
                            error: format!("unknown capability for llm_summarize: '{capability}'"),
                        });
                    }
                }
                ContextUpdateOp::Replace { .. } => {
                    problems.push(ValidationProblem {
                        path: format!("{state_path}.context_update.op"),
                        error: "'replace' is not allowed in preset hooks (only 'append' and 'struct_merge')".to_string(),
                    });
                }
                ContextUpdateOp::StructRemove { .. } => {
                    problems.push(ValidationProblem {
                        path: format!("{state_path}.context_update.op"),
                        error: "'struct_remove' is not allowed in preset hooks (only 'append' and 'struct_merge')".to_string(),
                    });
                }
            }
        }

        // V1.52 T-B P1: validate merge field
        if let Some(ref merge_kind) = state.merge {
            match merge_kind {
                crate::preset::manifest::MergeKind::All
                | crate::preset::manifest::MergeKind::Any => {
                    // All and Any are always valid.
                }
                crate::preset::manifest::MergeKind::Quorum { n, m } => {
                    if *n < 1 {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.merge.n"),
                            error: format!("quorum n must be >= 1, got {n}"),
                        });
                    }
                    if *n > *m {
                        problems.push(ValidationProblem {
                            path: format!("{state_path}.merge"),
                            error: format!("quorum n ({n}) must not exceed m ({m})"),
                        });
                    }
                }
            }
        }
    }

    // --- WS-E T6: Role validation ---
    // Build role ID set for agent reference validation
    let role_ids: HashSet<&str> = manifest.roles.iter().map(|r| r.id.as_str()).collect();

    // Check role ID uniqueness (already implicitly unique via HashSet,
    // but we want explicit error message)
    let mut seen_role_ids: HashSet<&str> = HashSet::new();
    for (i, role) in manifest.roles.iter().enumerate() {
        if seen_role_ids.contains(role.id.as_str()) {
            problems.push(ValidationProblem {
                path: format!("roles[{i}].id"),
                error: format!("duplicate role id: '{}'", role.id),
            });
        }
        seen_role_ids.insert(role.id.as_str());

        // Check recommended_skills format: must be a valid skill slug
        for (j, rec_skill) in role.recommended_skills.iter().enumerate() {
            if !validate_skill_slug_format(rec_skill) {
                problems.push(ValidationProblem {
                    path: format!("roles[{i}].recommended_skills[{j}]"),
                    error: format!(
                        "invalid recommended_skills format '{rec_skill}': expected lowercase alphanumeric with hyphens (e.g. 'novel-writing-assistant')"
                    ),
                });
            }
        }

        // Reject empty recommended_skills (loader should enforce at least one entry)
        if !role.recommended_skills.is_empty()
            && manifest
                .roles
                .iter()
                .any(|r| r.recommended_skills.is_empty())
        {
            // Only report if there are other roles with skills (mixed state)
            // If ALL roles have empty recommended_skills, that's a different error
        }
        if role.recommended_skills.is_empty() {
            problems.push(ValidationProblem {
                path: format!("roles[{i}].recommended_skills"),
                error: "role must have at least one recommended_skill".to_string(),
            });
        }
    }

    // Validate inner graphs
    if let Some(ref inner_graphs) = manifest.inner_graphs {
        for (name, ig) in inner_graphs {
            let ig_path = format!("inner_graphs.{name}");

            // Cycle detection on depends_on
            let cycle_path = ig_path.clone();
            if let Some(cycle) = detect_cycle(ig) {
                problems.push(ValidationProblem {
                    path: cycle_path,
                    error: format!("cycle detected: {cycle}"),
                });
            }

            // Check depends_on references
            let node_ids: HashSet<&str> = ig.nodes.iter().map(|n| n.id.as_str()).collect();
            for (k, node) in ig.nodes.iter().enumerate() {
                for dep in &node.depends_on {
                    if !node_ids.contains(dep.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{ig_path}.nodes[{k}].depends_on"),
                            error: format!("unknown node: '{dep}'"),
                        });
                    }
                }

                // WS-E T6: Check agent references
                if let Some(ref agent_ref) = node.agent {
                    // If roles are defined, agent must reference a valid role ID
                    if !manifest.roles.is_empty() && !role_ids.contains(agent_ref.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{ig_path}.nodes[{k}].agent"),
                            error: format!("unknown role reference: '{agent_ref}'"),
                        });
                    }
                    // If no roles defined, agent field should not be present
                    if manifest.roles.is_empty() {
                        problems.push(ValidationProblem {
                            path: format!("{ig_path}.nodes[{k}].agent"),
                            error: format!(
                                "agent field '{agent_ref}' references role, but no roles section defined"
                            ),
                        });
                    }
                }
            }

            // Check output_binding references a valid node
            if let Some(ref binding) = ig.output_binding {
                // output_binding format is "node_id.field", extract node_id
                let node_id = binding.split('.').next().unwrap_or(binding);
                if !node_ids.contains(node_id) {
                    problems.push(ValidationProblem {
                        path: format!("{ig_path}.output_binding"),
                        error: format!("output_binding references unknown node: '{node_id}'"),
                    });
                }
            }
        }
    }

    // --- WS-B T2: template_file path safety validation ---
    for (dot_path, template_file) in collect_template_file_entries(manifest) {
        if let Err(reason) = assert_template_file_safe(template_file) {
            problems.push(ValidationProblem {
                path: dot_path,
                error: reason,
            });
        }
    }

    problems
}

/// Detect a cycle in an inner graph's dependency edges.
///
/// Returns a human-readable cycle path if found, e.g. `"a → b → a"`.
fn detect_cycle(ig: &InnerGraph) -> Option<String> {
    // Build adjacency list: node -> list of nodes it points to.
    // depends_on: "this node depends on dep" → edge from node to dep
    // (we follow the depends_on direction for cycle detection)
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let node_ids: HashSet<&str> = ig.nodes.iter().map(|n| n.id.as_str()).collect();

    for node in &ig.nodes {
        for dep in &node.depends_on {
            if node_ids.contains(dep.as_str()) {
                adj.entry(&node.id).or_default().push(dep.as_str());
            }
        }
    }

    // DFS with three-color marking.
    let mut white: HashSet<&str> = node_ids.clone();
    let mut gray: HashSet<&str> = HashSet::new();
    let mut black: HashSet<&str> = HashSet::new();
    let mut path: Vec<&str> = Vec::new();

    for start in &node_ids {
        if white.contains(start) {
            if let Some(cycle) =
                dfs_cycle2(start, &adj, &mut white, &mut gray, &mut black, &mut path)
            {
                return Some(cycle);
            }
        }
    }

    None
}

fn dfs_cycle2<'a>(
    node: &'a str,
    adj: &HashMap<&'a str, Vec<&'a str>>,
    white: &mut HashSet<&'a str>,
    gray: &mut HashSet<&'a str>,
    black: &mut HashSet<&'a str>,
    path: &mut Vec<&'a str>,
) -> Option<String> {
    white.remove(node);
    gray.insert(node);
    path.push(node);

    if let Some(neighbors) = adj.get(node) {
        for next in neighbors {
            if black.contains(next) {
                continue;
            }
            if gray.contains(next) {
                // Found a cycle: path from next to node to next.
                let cycle_start = path.iter().position(|&n| n == *next).unwrap_or(0);
                let mut parts: Vec<String> = path[cycle_start..]
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
                parts.push(next.to_string());
                return Some(parts.join(" → "));
            }
            if let Some(cycle) = dfs_cycle2(next, adj, white, gray, black, path) {
                return Some(cycle);
            }
        }
    }

    gray.remove(node);
    black.insert(node);
    path.pop();
    None
}

/// Validate `recommended_skills` format: skill slug pattern.
///
/// Pattern: `^[a-z][a-z0-9-]*[a-z0-9]$` (lowercase alphanumeric + hyphens,
/// must start with letter, end with alphanumeric). Single-character slugs
/// (e.g. `"a"`) are also valid.
fn validate_skill_slug_format(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    // Must start with a lowercase letter
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    // Single char is valid
    if bytes.len() == 1 {
        return true;
    }
    // Must end with alphanumeric
    let last = bytes[bytes.len() - 1];
    if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
        return false;
    }
    // Middle chars: lowercase alphanumeric or hyphen
    bytes[1..bytes.len() - 1]
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

// ---------------------------------------------------------------------------
// Graph building per §8.2 mapping table
// ---------------------------------------------------------------------------

/// Build the outer state-machine graph per §8.2.
///
/// Each `states[].id` → a composite `Task` that encodes the enter actions,
/// `exit_when` condition, and terminal semantics.
///
/// Note: template resolution is skipped here because `build_outer_graph` is
/// used in test contexts where inline template strings are expected. Production
/// code uses `build_wired_outer_graph` which resolves `template_file` paths.
fn build_outer_graph(manifest: &PresetManifest) -> graph_flow::Graph {
    use crate::tasks::StateCompositeTask;
    use std::collections::HashMap;

    let graph = graph_flow::Graph::new(&manifest.preset.id);

    // V1.52 T-B P1: pre-compute incoming labeled edge counts for merge nodes.
    let mut incoming_labeled: HashMap<&str, usize> = HashMap::new();
    for state in &manifest.states {
        if let Some(crate::preset::manifest::NextTarget::Labeled(edges)) = &state.next {
            for edge in edges {
                *incoming_labeled.entry(edge.target.as_str()).or_insert(0) += 1;
            }
        }
        if let Some(crate::preset::manifest::NextTarget::GoNogo(gonogo)) = &state.next {
            *incoming_labeled.entry(gonogo.go.as_str()).or_insert(0) += 1;
            *incoming_labeled.entry(gonogo.nogo.as_str()).or_insert(0) += 1;
        }
    }

    for state in &manifest.states {
        let incoming = *incoming_labeled.get(state.id.as_str()).unwrap_or(&0);
        let task = StateCompositeTask::from_manifest(state).with_expected_incoming(incoming);
        graph.add_task(std::sync::Arc::new(task));
    }

    // Wire edges from state.next.
    for state in &manifest.states {
        match &state.next {
            Some(NextTarget::Linear(ref next_id)) => {
                graph.add_edge(&state.id, next_id);
            }
            Some(NextTarget::GoNogo(ref go_nogo)) => {
                // V1.42 P2: conditional edge reads _judge_result from context.
                // `go` branch when true; `nogo` branch when false or absent.
                graph.add_conditional_edge(
                    &state.id,
                    |ctx| ctx.get_sync::<bool>("_judge_result").unwrap_or(false),
                    &go_nogo.go,
                    &go_nogo.nogo,
                );
            }
            Some(NextTarget::Labeled(ref labeled_edges)) => {
                // V1.52 T-B P0: N-way labeled routing.
                // Each labeled edge gets a regular add_edge for reachability
                // validation. Actual routing is via NextAction::GoTo(target)
                // in StateCompositeTask::resolve_labeled_target, which also
                // writes the matched label to context._judge_label.
                for edge in labeled_edges {
                    graph.add_edge(&state.id, &edge.target);
                }
            }
            Some(NextTarget::Conditional(_) | NextTarget::Branches(_)) | None => {}
        }
    }

    graph
}

/// Build the outer graph with engine + inner graph references wired into
/// composite tasks (for `start_session_with_preset`).
///
/// `daemon_tool_dispatch` is passed by value because it's cloned into each
/// composite task that contains `HostTool` enter actions.
#[allow(clippy::needless_pass_by_value)]
pub fn build_wired_outer_graph(
    loaded: &LoadedPreset,
    engine: &Arc<dyn crate::engine::OrchestrationEngine>,
    caps: &Arc<CapabilityRegistry>,
    daemon_tool_dispatch: Option<std::sync::Arc<dyn crate::capability::DaemonToolDispatch>>,
) -> graph_flow::Graph {
    use crate::tasks::StateCompositeTask;
    use std::collections::HashMap;

    let graph = graph_flow::Graph::new(&loaded.id);

    // V1.52 T-B P1: pre-compute incoming labeled edge counts for merge nodes.
    let mut incoming_labeled: HashMap<&str, usize> = HashMap::new();
    for state in &loaded.manifest.states {
        if let Some(crate::preset::manifest::NextTarget::Labeled(edges)) = &state.next {
            for edge in edges {
                *incoming_labeled.entry(edge.target.as_str()).or_insert(0) += 1;
            }
        }
        if let Some(crate::preset::manifest::NextTarget::GoNogo(gonogo)) = &state.next {
            *incoming_labeled.entry(gonogo.go.as_str()).or_insert(0) += 1;
            *incoming_labeled.entry(gonogo.nogo.as_str()).or_insert(0) += 1;
        }
    }

    for state in &loaded.manifest.states {
        let incoming = *incoming_labeled.get(state.id.as_str()).unwrap_or(&0);
        let mut task = StateCompositeTask::from_manifest(state)
            .with_resolved_template(&loaded.id)
            .with_expected_incoming(incoming)
            .with_engine(engine.clone())
            .with_inner_graphs(loaded.inner_graphs.clone())
            .with_output_bindings(loaded.output_bindings.clone())
            .with_registry(caps.clone());

        // Wire daemon tool dispatch for HostTool enter actions (DF-47, V1.42 P3).
        if let Some(ref dispatch) = daemon_tool_dispatch {
            task = task.with_daemon_tool_dispatch(dispatch.clone());
        }

        graph.add_task(std::sync::Arc::new(task));
    }

    // Wire edges.
    for state in &loaded.manifest.states {
        match &state.next {
            Some(NextTarget::Linear(ref next_id)) => {
                graph.add_edge(&state.id, next_id);
            }
            Some(NextTarget::GoNogo(ref go_nogo)) => {
                graph.add_conditional_edge(
                    &state.id,
                    |ctx| ctx.get_sync::<bool>("_judge_result").unwrap_or(false),
                    &go_nogo.go,
                    &go_nogo.nogo,
                );
            }
            Some(NextTarget::Labeled(ref labeled_edges)) => {
                // V1.52 T-B P0: N-way labeled routing.
                for edge in labeled_edges {
                    graph.add_edge(&state.id, &edge.target);
                }
            }
            Some(NextTarget::Conditional(_) | NextTarget::Branches(_)) | None => {}
        }
    }

    graph
}

/// Extract output bindings from the manifest's `inner_graphs`.
fn extract_output_bindings(manifest: &PresetManifest) -> HashMap<String, String> {
    let mut bindings = HashMap::new();
    if let Some(ref inner_graphs) = manifest.inner_graphs {
        for (name, ig) in inner_graphs {
            if let Some(ref binding) = ig.output_binding {
                bindings.insert(name.clone(), binding.clone());
            }
        }
    }
    bindings
}

/// Build inner graphs per §8.2.
///
/// `inner_graphs.<name>.nodes[].kind=acp_prompt` → `AcpPromptTask` (stub in T3,
/// full in T4).
/// `inner_graphs.<name>.nodes[].depends_on` → `add_edge`.
///
/// ## WS-E T5: agent field propagation
///
/// Each node's `agent` field (if present) is stored in `InnerGraphNodeTask::agent_ref`.
/// At runtime, the engine resolves agent refs to `session_ids` and stores them
/// in context as `_session_routes`, which `InnerGraphNodeTask::run()` uses for routing.
fn build_inner_graphs(manifest: &PresetManifest) -> HashMap<String, Arc<graph_flow::Graph>> {
    use crate::preset::manifest::GraphNodeKind;
    use crate::tasks::InnerGraphNodeTask;

    let mut result = HashMap::new();

    if let Some(ref inner_graphs) = manifest.inner_graphs {
        for (name, ig) in inner_graphs {
            let graph = graph_flow::Graph::new(name);

            for node in &ig.nodes {
                // Determine kind (currently only acp_prompt supported).
                let task = match node.kind {
                    GraphNodeKind::AcpPrompt => {
                        InnerGraphNodeTask::new(&node.id)
                            // WS-E T5: store agent ref for runtime resolution
                            .with_agent_ref(node.agent.clone().unwrap_or_default())
                            // Tool policy from node (parse from string)
                            .with_tool_policy(
                                node.tool_policy
                                    .as_ref()
                                    .and_then(|s| std::str::FromStr::from_str(s.as_str()).ok())
                                    .unwrap_or(crate::tasks::ToolPolicy::AutoGrantReadOnly),
                            )
                            // Template file path (will be resolved at runtime)
                            .with_template(node.template_file.clone().unwrap_or_default())
                    }
                };
                graph.add_task(std::sync::Arc::new(task));
            }

            // Wire edges from depends_on
            for node in &ig.nodes {
                for dep in &node.depends_on {
                    graph.add_edge(dep, &node.id);
                }
            }

            result.insert(name.clone(), Arc::new(graph));
        }
    }

    result
}

// ---------------------------------------------------------------------------
// YAML depth measurement
// ---------------------------------------------------------------------------

/// Measure the maximum nesting depth of a [`serde_yaml::Value`].
///
/// Scalars have depth 1, sequences/mappings add 1 for the container level.
pub fn yaml_value_depth(value: &serde_yaml::Value) -> usize {
    match value {
        serde_yaml::Value::Mapping(map) => {
            let child_depth = map.values().map(yaml_value_depth).max().unwrap_or(0);
            let key_depth = map
                .keys()
                .filter_map(|k| match k {
                    serde_yaml::Value::Mapping(_) | serde_yaml::Value::Sequence(_) => {
                        Some(yaml_value_depth(k))
                    }
                    _ => None,
                })
                .max()
                .unwrap_or(0);
            1 + child_depth.max(key_depth)
        }
        serde_yaml::Value::Sequence(seq) => {
            let child_depth = seq.iter().map(yaml_value_depth).max().unwrap_or(0);
            1 + child_depth
        }
        _ => 1,
    }
}

// ---------------------------------------------------------------------------
// Unknown top-level key check (R-V137P0-01)
// ---------------------------------------------------------------------------

/// Known top-level keys in `preset.yaml` per the `PresetManifest` schema.
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &["preset", "states", "inner_graphs", "signals", "roles"];

/// Warn via `tracing::warn!` if the YAML document contains top-level keys
/// not recognized by [`PresetManifest`].
///
/// Catches mis-placed sections (e.g. `gates:` at root instead of under
/// `preset:`) that serde would silently ignore. The check is intentionally
/// **non-fatal** — existing embedded presets must not break. Callers that
/// want stricter behavior can promote the warnings to errors in a
/// follow-up iteration.
// Allow: first paragraph intentionally lists the purpose in full for
// single-reading callers of this helper; splitting would reduce clarity.
#[allow(clippy::too_long_first_doc_paragraph)]
pub fn warn_unknown_top_level_keys(yaml_value: &serde_yaml::Value) {
    let Some(mapping) = yaml_value.as_mapping() else {
        return;
    };
    for key in mapping.keys() {
        if let Some(key_str) = key.as_str() {
            if !KNOWN_TOP_LEVEL_KEYS.contains(&key_str) {
                tracing::warn!(
                    key = key_str,
                    "preset.yaml contains unknown top-level key — \
                     serde will silently ignore it; \
                     did you mean to nest it under `preset:`?"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal registry with a few test capabilities.
    fn test_capability_registry() -> CapabilityRegistry {
        CapabilityRegistry::with_builtins()
    }

    fn minimal_valid_yaml() -> &'static str {
        r"
preset:
  id: tiny
  version: 1
  kind: creator
  description: minimal
  requires_capabilities: []
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
    }

    #[test]
    fn valid_preset_loads_successfully() {
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(minimal_valid_yaml(), &caps).unwrap();
        assert_eq!(loaded.id, "tiny");
        assert_eq!(loaded.version, 1);
        assert!(!loaded.source_hash.is_empty());
    }

    #[test]
    fn reject_unknown_next_state() {
        let yaml = r"
preset:
  id: bad-next
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: does-not-exist
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("next") && p.error.contains("unknown state")),
            "expected 'unknown state' problem on next: {problems:?}"
        );
    }

    #[test]
    fn reject_missing_capability() {
        let yaml = r"
preset:
  id: bad-cap
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: capability
        name: nope.does.not.exist
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.error.contains("unknown capability")),
            "expected 'unknown capability' problem: {problems:?}"
        );
    }

    #[test]
    fn reject_inner_graph_cycle() {
        let yaml = r"
preset:
  id: cycle-test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: cyc
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
inner_graphs:
  cyc:
    nodes:
      - id: diverge
        kind: acp_prompt
        depends_on: [cluster]
      - id: cluster
        kind: acp_prompt
        depends_on: [diverge]
    output_binding: diverge.text
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems.iter().any(|p| p.error.contains("cycle")),
            "expected 'cycle' problem in inner_graphs: {problems:?}"
        );
    }

    #[test]
    fn reject_unknown_judge_capability() {
        let yaml = r"
preset:
  id: bad-judge
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      judge_capability: judge.nonexistent
    next: b
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.error.contains("unknown capability")),
            "expected 'unknown capability' for judge: {problems:?}"
        );
    }

    #[test]
    fn reject_conditional_next() {
        // V1.56 P2: Conditional next is now accepted on any state kind.
        let yaml = r#"
preset:
  id: cond-test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when: { kind: rule }
    next:
      kind: conditional
      rules:
        - when: "true"
          to: c
      default: b
  - id: b
    enter: []
    exit_when: { kind: manual }
    next: c
  - id: c
    terminal: true
"#;
        let caps = test_capability_registry();
        // V1.56 P2: conditional on non-llm_judge state is now accepted.
        let result = load_preset_from_str(yaml, &caps);
        assert!(
            result.is_ok(),
            "V1.56 P2: conditional next should be accepted on any state; got: {result:?}"
        );
    }

    #[test]
    fn reject_unknown_inner_graph_reference() {
        let yaml = r"
preset:
  id: bad-ig
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: nonexistent_graph
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.error.contains("unknown inner_graph")),
            "expected 'unknown inner_graph' problem: {problems:?}"
        );
    }

    #[test]
    fn reject_terminal_with_next() {
        let yaml = r"
preset:
  id: bad-terminal
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
    next: a
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("terminal") && p.error.contains("next")),
            "expected terminal state 'next' problem: {problems:?}"
        );
    }

    #[test]
    fn loaded_preset_has_outer_graph() {
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(minimal_valid_yaml(), &caps).unwrap();
        assert_eq!(loaded.outer_graph.id, "tiny");
        // Should have tasks for both states
        assert!(loaded.outer_graph.get_task("a").is_some());
        assert!(loaded.outer_graph.get_task("b").is_some());
    }

    #[test]
    fn loaded_preset_has_inner_graphs() {
        let yaml = r"
preset:
  id: ig-test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: my_graph
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
inner_graphs:
  my_graph:
    nodes:
      - id: n1
        kind: acp_prompt
      - id: n2
        kind: acp_prompt
        depends_on: [n1]
    output_binding: n2.text
";
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps).unwrap();
        assert!(loaded.inner_graphs.contains_key("my_graph"));
        let ig = &loaded.inner_graphs["my_graph"];
        assert!(ig.get_task("n1").is_some());
        assert!(ig.get_task("n2").is_some());
    }

    #[test]
    fn reject_unknown_depends_on_in_inner_graph() {
        let yaml = r"
preset:
  id: bad-dep
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: my_graph
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
inner_graphs:
  my_graph:
    nodes:
      - id: n1
        kind: acp_prompt
        depends_on: [nonexistent_node]
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems.iter().any(|p| p.error.contains("unknown node")),
            "expected 'unknown node' problem: {problems:?}"
        );
    }

    #[test]
    fn reject_invalid_initial_state() {
        let yaml = r"
preset:
  id: bad-init
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: nonexistent
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("initial") && p.error.contains("unknown state")),
            "expected 'unknown state' on initial: {problems:?}"
        );
    }

    #[test]
    fn reject_invalid_terminal_state() {
        let yaml = r"
preset:
  id: bad-term
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: nonexistent
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("terminal") && p.error.contains("unknown state")),
            "expected 'unknown state' on terminal: {problems:?}"
        );
    }

    #[test]
    fn valid_preset_with_known_capability_passes() {
        let yaml = r"
preset:
  id: cap-test
  version: 1
  kind: creator
  description: test
  requires_capabilities:
    - workspace.open
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: capability
        name: workspace.open
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps);
        assert!(loaded.is_ok(), "expected valid preset: {loaded:?}");
    }

    #[test]
    fn source_hash_is_deterministic() {
        let caps = test_capability_registry();
        let h1 = load_preset_from_str(minimal_valid_yaml(), &caps)
            .unwrap()
            .source_hash;
        let h2 = load_preset_from_str(minimal_valid_yaml(), &caps)
            .unwrap()
            .source_hash;
        assert_eq!(h1, h2);
    }

    #[test]
    fn source_hash_differs_for_different_yaml() {
        let caps = test_capability_registry();
        let h1 = load_preset_from_str(minimal_valid_yaml(), &caps)
            .unwrap()
            .source_hash;
        let yaml2 = r"
preset:
  id: other
  version: 1
  kind: creator
  description: minimal
  requires_capabilities: []
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
        let h2 = load_preset_from_str(yaml2, &caps).unwrap().source_hash;
        assert_ne!(h1, h2);
    }

    #[test]
    fn reject_unknown_requires_capabilities() {
        let yaml = r"
preset:
  id: bad-req-caps
  version: 1
  kind: creator
  description: test
  requires_capabilities:
    - workspace.open
    - capability.does_not_exist
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
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems.iter().any(|p| {
                p.path.contains("requires_capabilities")
                    && p.error.contains("capability.does_not_exist")
            }),
            "expected 'required capability not found' for unknown requires_capabilities entry: {problems:?}"
        );
    }

    #[test]
    fn known_requires_capabilities_passes() {
        let yaml = r"
preset:
  id: good-req-caps
  version: 1
  kind: creator
  description: test
  requires_capabilities:
    - workspace.open
    - sync.pull
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
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps);
        assert!(
            loaded.is_ok(),
            "expected valid preset with known requires_capabilities: {loaded:?}"
        );
    }

    // ── WS7 T4: initial_action + context_update parsing ──────────────────

    #[test]
    fn parse_initial_action_and_context_update() {
        use crate::preset::manifest::{ContextUpdateOp, InitialAction};

        let yaml = r"
preset:
  id: demo
  version: 1
  kind: creator
  description: demo
  requires_capabilities: []
  initial: a
  terminal: b
  initial_action:
    kind: seed_direct
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op: { kind: append }
      template_file: prompts/a-ctx.md
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps).unwrap();

        // Verify initial_action parsed
        assert!(matches!(
            loaded.initial_action,
            Some(InitialAction::SeedDirect)
        ));

        // Verify context_update hook on state "a"
        let hook = loaded.context_update_hooks.get("a").unwrap();
        assert_eq!(hook.template_file, "prompts/a-ctx.md");
        assert!(matches!(hook.op, ContextUpdateOp::Append { .. }));
    }

    #[test]
    fn parse_initial_action_seed_expansion() {
        use crate::preset::manifest::InitialAction;

        let yaml = r"
preset:
  id: exp-demo
  version: 1
  kind: creator
  description: demo
  requires_capabilities: []
  initial: a
  terminal: b
  initial_action:
    kind: seed_expansion
    capability: context.summarize
    template_file: prompts/seed-expand.md
    payload_kind: text
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps).unwrap();
        assert!(matches!(
            loaded.initial_action,
            Some(InitialAction::SeedExpansion { .. })
        ));
        if let Some(InitialAction::SeedExpansion {
            capability,
            template_file,
            payload_kind,
        }) = loaded.initial_action
        {
            assert_eq!(capability, "context.summarize");
            assert_eq!(template_file, Some("prompts/seed-expand.md".to_string()));
            assert_eq!(payload_kind, Some("text".to_string()));
        }
    }

    #[test]
    fn reject_context_update_with_replace_op() {
        let yaml = r#"
preset:
  id: bad-hook
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op: { kind: replace, body: "nope" }
      template_file: prompts/a-ctx.md
  - id: b
    terminal: true
"#;
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems.iter().any(|p| p.error.contains("replace")),
            "expected 'replace' problem in context_update: {problems:?}"
        );
    }

    #[test]
    fn reject_context_update_with_struct_remove_op() {
        let yaml = r#"
preset:
  id: bad-hook2
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op: { kind: struct_remove, path: "key" }
      template_file: prompts/a-ctx.md
  - id: b
    terminal: true
"#;
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems.iter().any(|p| p.error.contains("struct_remove")),
            "expected 'struct_remove' problem in context_update: {problems:?}"
        );
    }

    #[test]
    fn context_update_struct_merge_parses() {
        use crate::preset::manifest::ContextUpdateOp;

        let yaml = r"
preset:
  id: merge-demo
  version: 1
  kind: creator
  description: demo
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op: { kind: struct_merge }
      template_file: prompts/a-ctx.md
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps).unwrap();
        let hook = loaded.context_update_hooks.get("a").unwrap();
        assert!(matches!(hook.op, ContextUpdateOp::StructMerge { .. }));
    }

    #[test]
    fn preset_without_initial_action_loads_successfully() {
        // initial_action is optional — existing presets without it should still work.
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(minimal_valid_yaml(), &caps).unwrap();
        assert!(loaded.initial_action.is_none());
        assert!(loaded.context_update_hooks.is_empty());
    }

    #[test]
    fn context_update_llm_summarize_validates_known_capability() {
        let yaml = r"
preset:
  id: llm-sum-test
  version: 1
  kind: creator
  description: test llm_summarize context_update
  requires_capabilities:
    - context.summarize
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op:
        kind: llm_summarize
        capability: context.summarize
      template_file: prompts/summarize.md
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps).unwrap();
        let hook = loaded.context_update_hooks.get("a").unwrap();
        assert!(matches!(hook.op, ContextUpdateOp::LlmSummarize { .. }));
        if let ContextUpdateOp::LlmSummarize { capability } = &hook.op {
            assert_eq!(capability, "context.summarize");
        }
    }

    #[test]
    fn load_preset_from_directory_loads_valid_preset() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("tiny");
        std::fs::create_dir_all(&bundle_root).unwrap();
        let yaml_path = bundle_root.join("preset.yaml");
        std::fs::write(&yaml_path, minimal_valid_yaml()).unwrap();

        let caps = test_capability_registry();
        let loaded = load_preset(&bundle_root, &caps).unwrap();
        assert_eq!(loaded.id, "tiny");
        assert_eq!(loaded.version, 1);
    }

    #[test]
    fn load_preset_from_directory_rejects_invalid_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("bad-preset");
        std::fs::create_dir_all(&bundle_root).unwrap();
        std::fs::write(bundle_root.join("preset.yaml"), "not valid yaml: [").unwrap();

        let caps = test_capability_registry();
        let err = load_preset(&bundle_root, &caps).unwrap_err();
        assert!(
            matches!(&err, PresetLoadError::YamlParse(_)),
            "expected YAML parse error, got: {err:?}"
        );
    }

    #[test]
    fn load_preset_from_directory_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("missing");
        std::fs::create_dir_all(&bundle_root).unwrap();
        // No preset.yaml written

        let caps = test_capability_registry();
        let err = load_preset(&bundle_root, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.error.contains("failed to read preset.yaml")),
            "expected 'failed to read' error: {problems:?}"
        );
    }

    #[test]
    fn load_preset_from_directory_rejects_validation_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("invalid");
        std::fs::create_dir_all(&bundle_root).unwrap();
        let yaml = r"
preset:
  id: invalid
  version: 1
  kind: creator
  description: bad
  requires_capabilities: []
  initial: nonexistent
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        std::fs::write(bundle_root.join("preset.yaml"), yaml).unwrap();

        let caps = test_capability_registry();
        let err = load_preset(&bundle_root, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("initial") && p.error.contains("unknown state")),
            "expected validation error for unknown initial state: {problems:?}"
        );
    }

    // ── WS-B T1: assert_template_file_safe tests ───────────────

    #[test]
    fn template_file_safe_accepts_simple_relative_path() {
        assert!(assert_template_file_safe("prompts/system.md").is_ok());
    }

    #[test]
    fn template_file_safe_accepts_nested_relative_path() {
        assert!(assert_template_file_safe("a/b/c/template.md").is_ok());
    }

    #[test]
    fn template_file_safe_accepts_dot_filename() {
        assert!(assert_template_file_safe(".hidden").is_ok());
    }

    #[test]
    fn template_file_safe_rejects_dotdot_traversal() {
        assert!(assert_template_file_safe("../../etc/passwd").is_err());
    }

    #[test]
    fn template_file_safe_rejects_dotdot_via_sibling() {
        assert!(assert_template_file_safe("prompts/../secret").is_err());
    }

    #[test]
    fn template_file_safe_rejects_dotdot_mid_path() {
        assert!(assert_template_file_safe("a/../b/c.md").is_err());
    }

    #[test]
    fn template_file_safe_rejects_absolute_path() {
        assert!(assert_template_file_safe("/etc/passwd").is_err());
    }

    #[test]
    fn template_file_safe_rejects_backslash() {
        assert!(assert_template_file_safe("prompts\\windows\\path.md").is_err());
    }

    #[test]
    fn template_file_safe_rejects_null_byte() {
        assert!(assert_template_file_safe("prompts/bad\0file.md").is_err());
    }

    #[test]
    fn template_file_safe_rejects_control_characters() {
        assert!(assert_template_file_safe("prompts/bad\x01file.md").is_err());
    }

    #[test]
    fn template_file_safe_error_message_contains_path() {
        let err = assert_template_file_safe("../../etc/passwd").unwrap_err();
        assert!(err.contains("../../etc/passwd"));
        assert!(err.contains(".."));
    }

    #[test]
    fn context_update_llm_summarize_rejects_unknown_capability() {
        let yaml = r"
preset:
  id: llm-sum-bad
  version: 1
  kind: creator
  description: test llm_summarize with unknown capability
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op:
        kind: llm_summarize
        capability: capability.does_not_exist
      template_file: prompts/summarize.md
  - id: b
    terminal: true
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems.iter().any(|p| {
                p.path.contains("context_update")
                    && p.error.contains("unknown capability for llm_summarize")
            }),
            "expected validation error for unknown llm_summarize capability: {problems:?}"
        );
    }

    // ── WS-B T5: path traversal integration tests ──────────────

    #[test]
    fn reject_template_file_dotdot_traversal_in_exit_when() {
        let yaml = r#"
preset:
  id: traversal-exit
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
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
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("template_file") && p.error.contains("..")),
            "expected '..' traversal rejection in exit_when: {problems:?}"
        );
    }

    #[test]
    fn reject_template_file_absolute_path_in_context_update() {
        let yaml = r#"
preset:
  id: abs-path-hook
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op: { kind: append }
      template_file: "/absolute/path.md"
  - id: b
    terminal: true
"#;
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("template_file") && p.error.contains("absolute")),
            "expected absolute path rejection in context_update: {problems:?}"
        );
    }

    #[test]
    fn reject_template_file_dotdot_in_inner_graph_node() {
        let yaml = r#"
preset:
  id: traversal-ig
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: bad_ig
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
inner_graphs:
  bad_ig:
    nodes:
      - id: n1
        kind: acp_prompt
        template_file: "../../etc/passwd"
"#;
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("template_file") && p.error.contains("..")),
            "expected '..' traversal rejection in inner graph: {problems:?}"
        );
    }

    #[test]
    fn accept_valid_relative_template_file_paths() {
        let yaml = r#"
preset:
  id: valid-paths
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "prompts/system.md"
    next: b
  - id: b
    enter: []
    exit_when: { kind: manual }
    next: c
    context_update:
      op: { kind: append }
      template_file: "prompts/update.md"
  - id: c
    terminal: true
inner_graphs:
  my_ig:
    nodes:
      - id: n1
        kind: acp_prompt
        template_file: "prompts/node.md"
"#;
        let caps = test_capability_registry();
        let result = load_preset_from_str(yaml, &caps);
        assert!(result.is_ok(), "expected valid preset: {result:?}");
    }

    #[test]
    fn filesystem_preset_rejects_template_file_escaping_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("escape-test");
        std::fs::create_dir_all(&bundle_root).unwrap();
        // Create a real prompt file inside the bundle
        std::fs::create_dir_all(bundle_root.join("prompts")).unwrap();
        std::fs::write(bundle_root.join("prompts/good.md"), "hello").unwrap();
        // Create a symlink outside the bundle
        let outside = tmp.path().join("outside-secret.md");
        std::fs::write(&outside, "secret").unwrap();

        let yaml = r#"
preset:
  id: escape-test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "../outside-secret.md"
    next: b
  - id: b
    terminal: true
"#;
        std::fs::write(bundle_root.join("preset.yaml"), yaml).unwrap();
        let caps = test_capability_registry();
        let err = load_preset(&bundle_root, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems.iter().any(|p| p.error.contains("..")),
            "expected '..' structural rejection (load_preset_from_str catches this): {problems:?}"
        );
    }

    #[test]
    fn reject_template_file_null_byte() {
        assert!(assert_template_file_safe("prompts/bad\0file.md").is_err());
    }

    #[test]
    fn reject_template_file_control_char() {
        assert!(assert_template_file_safe("prompts/bad\x01file.md").is_err());
    }

    #[test]
    fn reject_template_file_dotdot_sibling_dir() {
        let yaml = r#"
preset:
  id: dotdot-sibling
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
    context_update:
      op: { kind: append }
      template_file: "prompts/../secret"
  - id: b
    terminal: true
"#;
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("template_file") && p.error.contains("..")),
            "expected '..' traversal rejection via sibling dir: {problems:?}"
        );
    }

    #[test]
    fn reject_template_file_dotdot_in_initial_action() {
        let yaml = r#"
preset:
  id: traversal-init
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
  initial_action:
    kind: seed_expansion
    capability: context.summarize
    template_file: "../../etc/passwd"
    payload_kind: text
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("initial_action") && p.error.contains("..")),
            "expected '..' traversal rejection in initial_action: {problems:?}"
        );
    }

    // ── R-M1-W04: YAML depth/size limit tests ───────────────────────

    #[test]
    fn reject_oversized_yaml() {
        // Build a YAML string that exceeds the size limit.
        let base = "preset:\n  id: big\n  version: 1\n  kind: creator\n  description: \"";
        let suffix = "\"\n  requires_capabilities: []\n  initial: a\n  terminal: b\nstates:\n  - id: a\n    enter: []\n    exit_when: { kind: manual }\n    next: b\n  - id: b\n    terminal: true\n";
        let padding_len = DEFAULT_MAX_YAML_SIZE + 1 - base.len() - suffix.len();
        let yaml = format!("{base}{}{suffix}", "x".repeat(padding_len));

        let caps = test_capability_registry();
        let err = load_preset_from_str_with_limits(
            &yaml,
            &caps,
            DEFAULT_MAX_YAML_SIZE,
            DEFAULT_MAX_YAML_DEPTH,
        )
        .unwrap_err();
        assert!(
            matches!(&err, PresetLoadError::YamlSizeExceeded { actual, limit } if *actual > *limit),
            "expected YamlSizeExceeded error: {err:?}"
        );
        // Verify actionable message content.
        let msg = format!("{err}");
        assert!(msg.contains("bytes"), "error should mention bytes: {msg}");
        assert!(
            msg.contains("Simplify"),
            "error should suggest simplification: {msg}"
        );
    }

    #[test]
    fn reject_deeply_nested_yaml() {
        use std::fmt::Write as _;

        // Build YAML with nesting deeper than the limit.
        let mut yaml = String::from("root:\n");
        for i in 1..=15 {
            let indent = "  ".repeat(i);
            writeln!(yaml, "{indent}level{i}:").expect("writing to String should not fail");
        }
        // Add valid preset structure at the top level to make it parseable.
        yaml.push_str("preset:\n  id: deep\n  version: 1\n  kind: creator\n  description: test\n  requires_capabilities: []\n  initial: a\n  terminal: b\nstates:\n  - id: a\n    enter: []\n    exit_when: { kind: manual }\n    next: b\n  - id: b\n    terminal: true\n");

        let caps = test_capability_registry();
        let err = load_preset_from_str_with_limits(
            &yaml,
            &caps,
            DEFAULT_MAX_YAML_SIZE,
            DEFAULT_MAX_YAML_DEPTH,
        )
        .unwrap_err();
        assert!(
            matches!(&err, PresetLoadError::YamlDepthExceeded { actual, limit } if *actual > *limit),
            "expected YamlDepthExceeded error: {err:?}"
        );
        let msg = format!("{err}");
        assert!(
            msg.contains("Flatten"),
            "error should suggest flattening: {msg}"
        );
    }

    #[test]
    fn normal_yaml_passes_size_and_depth_limits() {
        // Normal preset YAML should pass both checks without issue.
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(minimal_valid_yaml(), &caps);
        assert!(loaded.is_ok(), "normal preset should load: {loaded:?}");
    }

    #[test]
    fn yaml_value_depth_flat_scalar_is_1() {
        let val: serde_yaml::Value = serde_yaml::from_str("hello").unwrap();
        assert_eq!(yaml_value_depth(&val), 1);
    }

    #[test]
    fn yaml_value_depth_nested_mapping() {
        let val: serde_yaml::Value = serde_yaml::from_str("a:\n  b:\n    c: 1").unwrap();
        // root mapping → a mapping → b mapping → c scalar = 4
        assert_eq!(yaml_value_depth(&val), 4);
    }

    #[test]
    fn yaml_value_depth_sequence() {
        let val: serde_yaml::Value = serde_yaml::from_str("- a:\n    - b").unwrap();
        // root sequence → mapping → sequence → scalar = 4
        assert!(yaml_value_depth(&val) >= 3);
    }

    #[test]
    fn load_preset_from_str_with_limits_allows_higher_limits() {
        // Using higher limits should allow larger/deeper YAML.
        let yaml = minimal_valid_yaml();
        let caps = test_capability_registry();
        let loaded = load_preset_from_str_with_limits(yaml, &caps, 10 * 1024 * 1024, 50);
        assert!(
            loaded.is_ok(),
            "should pass with generous limits: {loaded:?}"
        );
    }

    // ── C-001 (rev2): A3 loader parity regression tests ───────────

    /// C-001 regression: `load_preset()` must reject a preset that references
    /// a template file which does not exist in the bundle directory.
    /// This exercises the `validate_assets_in_bundle()` A3 surface.
    #[test]
    fn a3_loader_rejects_missing_template_file_in_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("missing-asset");
        std::fs::create_dir_all(&bundle_root).unwrap();

        let yaml = r"preset:
  id: missing-asset
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
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
        std::fs::write(bundle_root.join("preset.yaml"), yaml).unwrap();

        let caps = test_capability_registry();
        let err = load_preset(&bundle_root, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.error.contains("nonexistent.md") && p.error.contains("does not exist")),
            "expected missing asset error from A3 surface: {problems:?}"
        );
    }

    /// C-001 regression: `load_preset()` must reject a preset that uses `..`
    /// traversal in `system_prompt_file` (role definition). This exercises
    /// the `validate_path_safety()` A3 surface for `prompt/system_prompt` paths.
    #[test]
    fn a3_loader_rejects_dotdot_in_system_prompt_file() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("traversal-role");
        std::fs::create_dir_all(&bundle_root).unwrap();

        let yaml = r#"preset:
  id: traversal-role
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
roles:
  - id: writer
    description: "Primary content writer"
    recommended_skills: [novel-writing-assistant]
    system_prompt_file: "../../etc/passwd"
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
        std::fs::write(bundle_root.join("preset.yaml"), yaml).unwrap();

        let caps = test_capability_registry();
        let err = load_preset(&bundle_root, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("system_prompt_file") && p.error.contains("..")),
            "expected '..' path safety error from A3 surface on system_prompt_file: {problems:?}"
        );
    }

    // ── V1.42 P2 T4: GoNogo conditional next tests ──────────────────────

    /// Helper YAML for a preset with `llm_judge` + `GoNogo` next.
    fn gonogo_yaml() -> &'static str {
        r#"
preset:
  id: gonogo-test
  version: 1
  kind: creator
  description: test gonogo conditional
  requires_capabilities: []
  run_intents: [work_init]
  initial: judge_state
  terminal: end
states:
  - id: judge_state
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "judge.txt"
    next:
      go: go_state
      nogo: nogo_state
  - id: go_state
    enter: []
    exit_when: { kind: manual }
    next: end
  - id: nogo_state
    enter: []
    exit_when: { kind: manual }
    next: end
  - id: end
    terminal: true
"#
    }

    #[test]
    fn gonogo_next_loads_successfully_on_llm_judge() {
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(gonogo_yaml(), &caps);
        assert!(loaded.is_ok(), "expected valid preset: {loaded:?}");
        let preset = loaded.unwrap();
        assert_eq!(preset.id, "gonogo-test");
    }

    #[test]
    fn gonogo_next_wires_conditional_edge() {
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(gonogo_yaml(), &caps).unwrap();

        // Verify outer graph has tasks for all four states.
        assert!(loaded.outer_graph.get_task("judge_state").is_some());
        assert!(loaded.outer_graph.get_task("go_state").is_some());
        assert!(loaded.outer_graph.get_task("nogo_state").is_some());
        assert!(loaded.outer_graph.get_task("end").is_some());

        // When _judge_result is true, find_next_task should return go_state.
        let ctx = graph_flow::Context::new();
        ctx.set_sync("_judge_result", true);
        let next = loaded.outer_graph.find_next_task("judge_state", &ctx);
        assert_eq!(
            next.as_deref(),
            Some("go_state"),
            "GO path: expected go_state, got {next:?}"
        );

        // When _judge_result is false, find_next_task should return nogo_state.
        let ctx2 = graph_flow::Context::new();
        ctx2.set_sync("_judge_result", false);
        let next2 = loaded.outer_graph.find_next_task("judge_state", &ctx2);
        assert_eq!(
            next2.as_deref(),
            Some("nogo_state"),
            "NOGO path: expected nogo_state, got {next2:?}"
        );

        // When _judge_result is absent, find_next_task should return nogo_state (fallback).
        let ctx3 = graph_flow::Context::new();
        let next3 = loaded.outer_graph.find_next_task("judge_state", &ctx3);
        assert_eq!(
            next3.as_deref(),
            Some("nogo_state"),
            "No judge result: expected nogo_state fallback, got {next3:?}"
        );
    }

    #[test]
    fn reject_gonogo_on_non_llm_judge_state() {
        let yaml = r"
preset:
  id: bad-gonogo
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next:
      go: b
      nogo: c
  - id: b
    enter: []
    exit_when: { kind: manual }
    next: c
  - id: c
    terminal: true
";
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.error.contains("go/nogo") && p.error.contains("llm_judge")),
            "expected 'go/nogo only valid on llm_judge' problem: {problems:?}"
        );
    }

    #[test]
    fn reject_gonogo_with_unknown_go_target() {
        let yaml = r#"
preset:
  id: bad-gonogo-go
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "judge.txt"
    next:
      go: nonexistent
      nogo: c
  - id: c
    terminal: true
"#;
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("next.go") && p.error.contains("unknown state")),
            "expected 'unknown state' on next.go: {problems:?}"
        );
    }

    #[test]
    fn reject_gonogo_with_unknown_nogo_target() {
        let yaml = r#"
preset:
  id: bad-gonogo-nogo
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      template_file: "judge.txt"
    next:
      go: c
      nogo: nonexistent
  - id: c
    terminal: true
"#;
        let caps = test_capability_registry();
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.path.contains("next.nogo") && p.error.contains("unknown state")),
            "expected 'unknown state' on next.nogo: {problems:?}"
        );
    }

    #[test]
    fn expression_conditional_still_rejected() {
        // V1.56 P2: The expression-based Conditional form is now accepted on llm_judge states.
        // This test verifies the form parses correctly (targets must be valid state IDs).
        let yaml = r#"
preset:
  id: expr-cond
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
    next:
      kind: conditional
      rules:
        - when: "true"
          target: b
      default: c
  - id: b
    enter: []
    exit_when: { kind: manual }
    next: c
  - id: c
    terminal: true
"#;
        let caps = test_capability_registry();
        // V1.56 P2: conditional form now accepted; validates target state references.
        let result = load_preset_from_str(yaml, &caps);
        assert!(
            result.is_ok(),
            "V1.56 P2: expression conditional should be accepted; got: {result:?}"
        );
    }

    // R-V137P0-01: unknown top-level key detection.
    #[test]
    fn warn_unknown_top_level_keys_detects_misplaced_gates() {
        use std::sync::{Arc, Mutex};

        // Capturing layer to assert that tracing::warn! is actually emitted.
        #[derive(Clone)]
        struct CaptureLayer {
            messages: Arc<Mutex<Vec<String>>>,
        }
        impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
            fn on_event(
                &self,
                event: &tracing::Event<'_>,
                _ctx: tracing_subscriber::layer::Context<'_, S>,
            ) {
                if event.metadata().level() == &tracing::Level::WARN {
                    let mut visitor = CaptureVisitor(String::new());
                    event.record(&mut visitor);
                    let mut msgs = self.messages.lock().unwrap();
                    msgs.push(visitor.0);
                }
            }
        }
        struct CaptureVisitor(String);
        impl tracing::field::Visit for CaptureVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                use std::fmt::Write;
                let _ = write!(&mut self.0, "{}={:?} ", field.name(), value);
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                use std::fmt::Write;
                let _ = write!(&mut self.0, "{}={} ", field.name(), value);
            }
        }

        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let layer = CaptureLayer {
            messages: captured.clone(),
        };
        let subscriber =
            <tracing_subscriber::Registry as tracing_subscriber::layer::SubscriberExt>::with(
                tracing_subscriber::registry::Registry::default(),
                layer,
            );

        tracing::subscriber::with_default(subscriber, || {
            let yaml = r"
preset:
  id: stray-keys-test
  version: 1
  kind: creator
  description: test
  initial: start
  terminal: done
states:
  - id: start
    next: done
  - id: done
    terminal: true
gates:
  - kind: file_exists
    path: Works/{{work_ref}}/README.md
";
            let caps = test_capability_registry();
            // Should NOT fail — unknown keys are warnings only.
            let loaded = load_preset_from_str(yaml, &caps).unwrap();
            assert_eq!(loaded.id, "stray-keys-test");

            // Also call the helper directly to ensure the warn path fires.
            let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
            super::warn_unknown_top_level_keys(&yaml_value);
        });

        {
            let messages = captured.lock().unwrap();
            assert!(
                messages.iter().any(|m| m.contains("gates")),
                "expected tracing::warn! mentioning 'gates', got: {messages:?}"
            );
            drop(messages);
        }
    }
}
