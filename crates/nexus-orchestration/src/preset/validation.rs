//! Shared semantic preset validation facade (V1.32 P1).
//!
//! Single validation surface used by:
//! - Runtime loader (`load_preset_from_str` / `load_preset`)
//! - Daemon `POST /v1/local/presets:validate`
//!
//! Extends the structural checks in `loader::validate_manifest` with:
//! - A2: Logical completeness (reachability, terminal consistency, id match, orphan inner graphs)
//! - A3: Asset and path safety (file existence in bundle, prompt/template path resolution)
//! - A4: Capability compatibility (registry lookup, argument drift)
//!
//! ## Architect decision: orphan inner graphs = WARNING
//!
//! An inner graph that is defined but never referenced by any state's `enter`
//! action produces a WARNING diagnostic, not an error. This allows presets to
//! define utility graphs for future use without breaking validation. The caller
//! may elevate to error in strict mode (future work).

use crate::capability::CapabilityRegistry;
use crate::preset::manifest::{
    EnterAction, ExitWhen, MergeKind, NextTarget, PresetKind, PresetManifest, RunIntent,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;

// ---------------------------------------------------------------------------
// Diagnostic types
// ---------------------------------------------------------------------------

/// Severity of a validation diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    /// Hard error — preset will not load.
    Error,
    /// Soft warning — preset loads but may not behave as intended.
    Warning,
}

/// A single validation diagnostic with actionable detail.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidationDiagnostic {
    /// Dot-path to the offending field (e.g. `states[1].enter[0].name`).
    pub path: String,
    /// Human-readable description of the problem.
    pub message: String,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Machine-readable category for consumers that want to filter.
    pub category: DiagnosticCategory,
}

/// Machine-readable diagnostic category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    /// Structural issue (missing/unknown field reference).
    Structural,
    /// State machine reachability issue.
    Reachability,
    /// Terminal state marker/header mismatch.
    TerminalConsistency,
    /// Bundle directory id vs manifest id mismatch.
    IdMismatch,
    /// Inner graph defined but never referenced.
    OrphanInnerGraph,
    /// Referenced asset file missing from bundle.
    MissingAsset,
    /// Path escapes the bundle sandbox.
    PathSafety,
    /// Capability name unknown to the registry.
    MissingCapability,
    /// Capability argument drift (unknown or missing args).
    CapabilityArgDrift,
    /// Schema check could not be performed (registry lacks metadata).
    SchemaCheckSkipped,
    /// Run intent classification issue (V1.33 §5).
    RunIntents,
    /// V1.52 T-B P0: duplicate label values in labeled next edges.
    DuplicateLabel,
    /// V1.52 T-B P1: merge node integrity violations.
    MergeIntegrity,
}

/// Result of semantic validation: a list of diagnostics.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ValidationResult {
    /// All diagnostics (errors + warnings).
    pub diagnostics: Vec<ValidationDiagnostic>,
}

impl ValidationResult {
    /// Returns `true` if there are any error-severity diagnostics.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error)
    }

    /// Returns only error-severity diagnostics.
    pub fn errors(&self) -> impl Iterator<Item = &ValidationDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
    }

    /// Returns only warning-severity diagnostics.
    pub fn warnings(&self) -> impl Iterator<Item = &ValidationDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
    }

    /// Convert to the legacy `ValidationProblem` format used by `PresetLoadError`.
    #[must_use]
    pub fn to_problems(&self) -> Vec<super::loader::ValidationProblem> {
        self.diagnostics
            .iter()
            .map(|d| super::loader::ValidationProblem {
                path: d.path.clone(),
                error: d.message.clone(),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Public validation entry point
// ---------------------------------------------------------------------------

/// Run all semantic validation checks (A1–A4) against a parsed manifest.
///
/// This is the **single shared validation surface** for both the runtime
/// loader and the daemon/API validate endpoint. Callers that need
/// filesystem-level checks (template file existence) should also call
/// [`validate_assets_in_bundle`] with the bundle root path.
///
/// # Arguments
///
/// * `manifest` — The parsed `PresetManifest` to validate.
/// * `caps` — The capability registry for name/argument checks.
///
/// # Returns
///
/// A `ValidationResult` containing all diagnostics (errors + warnings).
/// The caller decides whether to reject on errors and how to present warnings.
#[must_use]
pub fn validate_preset_semantic(
    manifest: &PresetManifest,
    caps: &CapabilityRegistry,
) -> ValidationResult {
    let mut result = ValidationResult::default();

    // A2: Logical completeness checks
    check_initial_to_terminal_reachability(manifest, &mut result);
    check_terminal_marker_consistency(manifest, &mut result);
    check_bundle_id_match(manifest, &mut result);
    check_inner_graph_references(manifest, &mut result);
    check_labeled_edge_duplicates(manifest, &mut result);
    check_merge_node_integrity(manifest, &mut result);

    // A4: Capability compatibility checks
    check_capability_arg_compatibility(manifest, caps, &mut result);

    // A5: Run intents checks (V1.33 §5)
    check_run_intents(manifest, &mut result);

    // A6: CLI args checks (V1.45 §3.3)
    check_cli_args(manifest, &mut result);

    result
}

/// Run asset-path checks against a bundle root directory (A3).
///
/// Verifies that all `template_file`, `prompt_file`, and `system_prompt_file`
/// references resolve to files that exist within the bundle sandbox.
///
/// # Arguments
///
/// * `manifest` — The parsed `PresetManifest`.
/// * `bundle_root` — The filesystem root of the preset bundle.
#[must_use]
pub fn validate_assets_in_bundle(
    manifest: &PresetManifest,
    bundle_root: &Path,
) -> ValidationResult {
    let mut result = ValidationResult::default();
    check_bundle_id_vs_directory(manifest, bundle_root, &mut result);
    check_asset_file_existence(manifest, bundle_root, &mut result);
    check_symlink_escapes(manifest, bundle_root, &mut result);
    result
}

/// Run path-safety structural checks against a manifest (A3 — shared with loader).
///
/// Validates that all `template_file`, `prompt_file`, and `system_prompt_file`
/// references are safe relative paths (no `..`, no absolute paths, no backslashes,
/// no null bytes, no control characters). Returns diagnostics for each unsafe path.
#[must_use]
pub fn validate_path_safety(manifest: &PresetManifest) -> ValidationResult {
    let mut result = ValidationResult::default();
    check_path_safety(manifest, &mut result);
    result
}

// ---------------------------------------------------------------------------
// A2: Logical completeness checks
// ---------------------------------------------------------------------------

/// Check that the initial state can reach at least one terminal state via
/// the outer graph's `next` edges.
fn check_initial_to_terminal_reachability(
    manifest: &PresetManifest,
    result: &mut ValidationResult,
) {
    let state_ids: HashSet<&str> = manifest.states.iter().map(|s| s.id.as_str()).collect();

    // Build adjacency list from next edges (linear + GoNogo branches).
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for state in &manifest.states {
        match &state.next {
            Some(crate::preset::manifest::NextTarget::Linear(target)) => {
                if state_ids.contains(target.as_str()) {
                    adj.entry(&state.id).or_default().push(target.as_str());
                }
            }
            Some(crate::preset::manifest::NextTarget::GoNogo(gonogo)) => {
                if state_ids.contains(gonogo.go.as_str()) {
                    adj.entry(&state.id).or_default().push(gonogo.go.as_str());
                }
                if state_ids.contains(gonogo.nogo.as_str()) {
                    adj.entry(&state.id).or_default().push(gonogo.nogo.as_str());
                }
            }
            Some(crate::preset::manifest::NextTarget::Labeled(labeled)) => {
                // V1.52 T-B P0: N-way labeled edges — each edge's target
                // is a reachable state from the source.
                for edge in labeled {
                    if state_ids.contains(edge.target.as_str()) {
                        adj.entry(&state.id).or_default().push(edge.target.as_str());
                    }
                }
            }
            _ => {}
        }
    }

    // Find terminal states (states with terminal: true or no next AND no exit_when
    // that could keep them running). For our purposes, a state is terminal if
    // `terminal: true` or `next` is absent.
    let terminal_states: HashSet<&str> = manifest
        .states
        .iter()
        .filter(|s| s.terminal || s.next.is_none())
        .map(|s| s.id.as_str())
        .collect();

    if terminal_states.is_empty() {
        result.diagnostics.push(ValidationDiagnostic {
            path: "states".to_string(),
            message: "no terminal state found: at least one state must be terminal".to_string(),
            severity: DiagnosticSeverity::Error,
            category: DiagnosticCategory::Reachability,
        });
        return;
    }

    // BFS from initial to any terminal state.
    let initial = manifest.preset.initial.as_str();
    if !state_ids.contains(initial) {
        // Already caught by loader validation; skip here to avoid duplicate.
        return;
    }

    let mut visited: HashSet<&str> = HashSet::new();
    let mut queue: std::collections::VecDeque<&str> = std::collections::VecDeque::new();
    queue.push_back(initial);
    visited.insert(initial);

    let mut reachable_terminal = false;
    while let Some(current) = queue.pop_front() {
        if terminal_states.contains(current) {
            reachable_terminal = true;
            break;
        }
        if let Some(neighbors) = adj.get(current) {
            for &next in neighbors {
                if visited.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }

    if !reachable_terminal {
        result.diagnostics.push(ValidationDiagnostic {
            path: format!("preset.initial ({initial})"),
            message: format!(
                "initial state '{initial}' cannot reach any terminal state via 'next' edges"
            ),
            severity: DiagnosticSeverity::Error,
            category: DiagnosticCategory::Reachability,
        });
    }
}

/// Check terminal marker/header consistency.
///
/// The header field `preset.terminal` names the intended terminal state.
/// States with `terminal: true` are the actual terminal markers.
/// These must agree: the state named by `preset.terminal` must have `terminal: true`,
/// and there should be no other states marked `terminal: true` unless they are
/// unreachable (which is a separate reachability concern).
fn check_terminal_marker_consistency(manifest: &PresetManifest, result: &mut ValidationResult) {
    let declared_terminal = manifest.preset.terminal.as_str();

    for (i, state) in manifest.states.iter().enumerate() {
        if state.id == declared_terminal && !state.terminal {
            result.diagnostics.push(ValidationDiagnostic {
                path: format!("states[{i}].terminal"),
                message: format!(
                    "state '{declared_terminal}' is declared as preset.terminal but is not marked terminal: true"
                ),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::TerminalConsistency,
            });
        }
        // States marked terminal that are NOT the declared terminal: warning only.
        // They won't cause a runtime error, but may indicate authoring confusion.
        if state.terminal && state.id != declared_terminal {
            result.diagnostics.push(ValidationDiagnostic {
                path: format!("states[{i}].terminal"),
                message: format!(
                    "state '{}' is marked terminal: true but is not the declared preset.terminal ('{declared_terminal}')",
                    state.id
                ),
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::TerminalConsistency,
            });
        }
    }
}

/// Check that the manifest id matches the expected bundle directory id.
///
/// This is a semantic check that the `preset.id` field is consistent.
/// When loading from a bundle directory, the caller should compare the
/// directory name with `manifest.preset.id`. This function checks the
/// manifest id is a valid non-empty slug.
fn check_bundle_id_match(manifest: &PresetManifest, result: &mut ValidationResult) {
    let id = &manifest.preset.id;
    if id.is_empty() {
        result.diagnostics.push(ValidationDiagnostic {
            path: "preset.id".to_string(),
            message: "preset.id must not be empty".to_string(),
            severity: DiagnosticSeverity::Error,
            category: DiagnosticCategory::IdMismatch,
        });
        return;
    }

    // Validate id format: lowercase alphanumeric, dots, hyphens, underscores.
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_')
    {
        result.diagnostics.push(ValidationDiagnostic {
            path: "preset.id".to_string(),
            message: format!(
                "preset.id '{id}' contains invalid characters (expected: lowercase alphanumeric, '.', '-', '_')"
            ),
            severity: DiagnosticSeverity::Error,
            category: DiagnosticCategory::IdMismatch,
        });
    }

    if !id.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        result.diagnostics.push(ValidationDiagnostic {
            path: "preset.id".to_string(),
            message: format!("preset.id '{id}' must start with a lowercase letter"),
            severity: DiagnosticSeverity::Error,
            category: DiagnosticCategory::IdMismatch,
        });
    }
}

/// Check inner graph references:
/// - Referenced inner graphs must exist (already in loader validation, but we add
///   a consistent diagnostic here).
/// - Orphan inner graphs (defined but never referenced) produce a WARNING.
fn check_inner_graph_references(manifest: &PresetManifest, result: &mut ValidationResult) {
    let inner_graph_names: HashSet<&str> = manifest
        .inner_graphs
        .as_ref()
        .map(|igs| igs.keys().map(std::string::String::as_str).collect())
        .unwrap_or_default();

    // Collect all referenced inner graph names from enter actions.
    let mut referenced: HashSet<&str> = HashSet::new();
    for state in &manifest.states {
        for enter in &state.enter {
            if let EnterAction::InnerGraph { name } = enter {
                referenced.insert(name.as_str());
            }
        }
    }

    // Find missing references: referenced but not defined.
    for name in &referenced {
        if !inner_graph_names.contains(name) {
            result.diagnostics.push(ValidationDiagnostic {
                path: format!("enter.kind=inner_graph name={name}"),
                message: format!("referenced inner graph '{name}' is not defined"),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::Structural,
            });
        }
    }

    // Short-circuit: no inner graphs defined => nothing more to check for orphans.
    if inner_graph_names.is_empty() {
        return;
    }

    // Find orphans: defined but not referenced.
    for name in &inner_graph_names {
        if !referenced.contains(name) {
            // Architect decision: orphan inner graphs = WARNING, not error.
            // Allows presets to define utility graphs for future use.
            result.diagnostics.push(ValidationDiagnostic {
                path: format!("inner_graphs.{name}"),
                message: format!(
                    "inner graph '{name}' is defined but not referenced by any state's enter action"
                ),
                severity: DiagnosticSeverity::Warning,
                category: DiagnosticCategory::OrphanInnerGraph,
            });
        }
    }
}

/// V1.52 T-B P0/P1: check for duplicate label values in labeled next edges.
///
/// Two checks:
/// 1. **Within-state**: two edges in the same state with the same label.
/// 2. **Cross-state** (W-QC3-1): two different states emitting the same label
///    targeting the same merge node. The runtime dedupes by label, so
///    duplicates from different sources would prevent `arrived_count` from
///    ever reaching `expected_incoming` — a silent session stall.
fn check_labeled_edge_duplicates(manifest: &PresetManifest, result: &mut ValidationResult) {
    // Within-state check.
    for (i, state) in manifest.states.iter().enumerate() {
        if let Some(NextTarget::Labeled(edges)) = &state.next {
            let mut seen_labels: HashSet<&str> = HashSet::new();
            for (k, edge) in edges.iter().enumerate() {
                if !seen_labels.insert(&edge.label) {
                    result.diagnostics.push(ValidationDiagnostic {
                        path: format!("states[{i}].next[{k}].label"),
                        message: format!(
                            "duplicate label '{}' in labeled next edges of state '{}'",
                            edge.label, state.id
                        ),
                        severity: DiagnosticSeverity::Error,
                        category: DiagnosticCategory::DuplicateLabel,
                    });
                }
            }
        }
    }

    // W-QC3-1: Cross-state check — for each (target, label) pair, verify
    // no two source states share the same label targeting the same merge node.
    let mut merge_labels: HashMap<(&str, &str), &str> = HashMap::new();
    for (i, state) in manifest.states.iter().enumerate() {
        if let Some(NextTarget::Labeled(edges)) = &state.next {
            for (k, edge) in edges.iter().enumerate() {
                let key = (edge.target.as_str(), edge.label.as_str());
                if let Some(&existing_source) = merge_labels.get(&key) {
                    result.diagnostics.push(ValidationDiagnostic {
                        path: format!("states[{i}].next[{k}].label"),
                        message: format!(
                            "duplicate label '{}' targeting merge node '{}' from state '{}' \
                             (also emitted by state '{existing_source}')",
                            edge.label, edge.target, state.id
                        ),
                        severity: DiagnosticSeverity::Error,
                        category: DiagnosticCategory::DuplicateLabel,
                    });
                } else {
                    merge_labels.insert(key, state.id.as_str());
                }
            }
        }
    }
}

/// V1.52 T-B P1: validate merge node integrity.
///
/// Checks:
/// - Each state with `merge:` must have ≥2 incoming labeled edges.
/// - For `quorum N/M`: N ≥ 1 and N ≤ M.
/// - M must equal the count of incoming labeled edges.
/// - Merge nodes must have at least one outgoing edge.
fn check_merge_node_integrity(manifest: &PresetManifest, result: &mut ValidationResult) {
    // Count incoming labeled edges per state.
    let mut incoming_labeled: HashMap<&str, usize> = HashMap::new();
    for state in &manifest.states {
        if let Some(NextTarget::Labeled(edges)) = &state.next {
            for edge in edges {
                *incoming_labeled.entry(edge.target.as_str()).or_insert(0) += 1;
            }
        }
    }

    // Also count GoNogo edges (they also converge on merge nodes).
    for state in &manifest.states {
        if let Some(NextTarget::GoNogo(gonogo)) = &state.next {
            *incoming_labeled.entry(gonogo.go.as_str()).or_insert(0) += 1;
            *incoming_labeled.entry(gonogo.nogo.as_str()).or_insert(0) += 1;
        }
    }

    for (i, state) in manifest.states.iter().enumerate() {
        let Some(merge) = &state.merge else {
            continue;
        };

        let state_path = format!("states[{i}]");
        let incoming = *incoming_labeled.get(state.id.as_str()).unwrap_or(&0);

        // Must have ≥2 incoming labeled edges.
        if incoming < 2 {
            result.diagnostics.push(ValidationDiagnostic {
                path: format!("{state_path}.merge"),
                message: format!(
                    "merge node '{}' requires at least 2 incoming labeled edges, found {incoming}",
                    state.id
                ),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::MergeIntegrity,
            });
            continue;
        }

        // Quorum: validate N/M bounds.
        if let MergeKind::Quorum { n, m } = merge {
            if *n < 1 {
                result.diagnostics.push(ValidationDiagnostic {
                    path: format!("{state_path}.merge.n"),
                    message: format!("quorum merge node '{}' n must be >= 1, got {n}", state.id),
                    severity: DiagnosticSeverity::Error,
                    category: DiagnosticCategory::MergeIntegrity,
                });
            }
            if *n > *m {
                result.diagnostics.push(ValidationDiagnostic {
                    path: format!("{state_path}.merge"),
                    message: format!(
                        "quorum merge node '{}' n ({n}) must not exceed m ({m})",
                        state.id
                    ),
                    severity: DiagnosticSeverity::Error,
                    category: DiagnosticCategory::MergeIntegrity,
                });
            }
            if *m != incoming {
                result.diagnostics.push(ValidationDiagnostic {
                    path: format!("{state_path}.merge.m"),
                    message: format!(
                        "quorum merge node '{}' declared m ({m}) does not match actual incoming labeled edge count ({incoming})",
                        state.id
                    ),
                    severity: DiagnosticSeverity::Error,
                    category: DiagnosticCategory::MergeIntegrity,
                });
            }
        }

        // Merge nodes must have at least one outgoing edge (not terminal without next).
        if state.next.is_none() && !state.terminal {
            result.diagnostics.push(ValidationDiagnostic {
                path: format!("{state_path}.merge"),
                message: format!(
                    "merge node '{}' has no outgoing next edge and is not marked terminal",
                    state.id
                ),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::MergeIntegrity,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// A3: Asset and path safety checks
// ---------------------------------------------------------------------------

/// Collect all file-backed asset references from a manifest.
///
/// Returns `(dot_path, relative_path)` pairs for `template_file`, `prompt_file`,
/// and `system_prompt_file` references.
pub(crate) fn collect_asset_file_references(manifest: &PresetManifest) -> Vec<(String, String)> {
    let mut refs: Vec<(String, String)> = Vec::new();

    // Template file references (exit_when, context_update, inner_graph nodes, initial_action)
    // are already collected by loader::collect_template_file_entries, but that's private.
    // We re-collect here for independence.

    // ExitWhen template_file
    for (i, state) in manifest.states.iter().enumerate() {
        if let Some(ExitWhen::LlmJudge {
            template_file: Some(ref tf),
            ..
        }) = state.exit_when
        {
            refs.push((format!("states[{i}].exit_when.template_file"), tf.clone()));
        }

        // context_update template_file
        if let Some(ref hook) = state.context_update {
            refs.push((
                format!("states[{i}].context_update.template_file"),
                hook.template_file.clone(),
            ));
        }

        // Enter action prompt_file / system_prompt_file
        for (j, enter) in state.enter.iter().enumerate() {
            if let EnterAction::Capability {
                args: Some(args_val),
                ..
            } = enter
            {
                if let Some(pf) = args_val.get("prompt_file").and_then(|v| v.as_str()) {
                    refs.push((
                        format!("states[{i}].enter[{j}].args.prompt_file"),
                        pf.to_string(),
                    ));
                }
                if let Some(spf) = args_val.get("system_prompt_file").and_then(|v| v.as_str()) {
                    refs.push((
                        format!("states[{i}].enter[{j}].args.system_prompt_file"),
                        spf.to_string(),
                    ));
                }
            }
        }
    }

    // Inner graph node template_file
    if let Some(ref inner_graphs) = manifest.inner_graphs {
        for (ig_name, ig) in inner_graphs {
            for (k, node) in ig.nodes.iter().enumerate() {
                if let Some(ref tf) = node.template_file {
                    refs.push((
                        format!("inner_graphs.{ig_name}.nodes[{k}].template_file"),
                        tf.clone(),
                    ));
                }
            }
        }
    }

    // initial_action template_file
    if let Some(crate::preset::manifest::InitialAction::SeedExpansion {
        template_file: Some(ref tf),
        ..
    }) = manifest.preset.initial_action
    {
        refs.push((
            "preset.initial_action.template_file".to_string(),
            tf.clone(),
        ));
    }

    // Role system_prompt_file
    for (i, role) in manifest.roles.iter().enumerate() {
        refs.push((
            format!("roles[{i}].system_prompt_file"),
            role.system_prompt_file.clone(),
        ));
    }

    refs
}

/// Check that all file-backed asset references exist within the bundle.
fn check_asset_file_existence(
    manifest: &PresetManifest,
    bundle_root: &Path,
    result: &mut ValidationResult,
) {
    let refs = collect_asset_file_references(manifest);

    for (dot_path, rel_path) in &refs {
        // Skip structural checks (already done by assert_template_file_safe in loader).
        // Only check existence here.
        let full_path = bundle_root.join(rel_path);
        if !full_path.exists() {
            result.diagnostics.push(ValidationDiagnostic {
                path: dot_path.clone(),
                message: format!(
                    "referenced file '{rel_path}' does not exist in bundle at {}",
                    bundle_root.display()
                ),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::MissingAsset,
            });
        }
    }
}

/// Check that no asset references resolve via symlinks to locations outside
/// the bundle sandbox. This is defense-in-depth beyond the structural `..`
/// check already in the loader.
fn check_symlink_escapes(
    manifest: &PresetManifest,
    bundle_root: &Path,
    result: &mut ValidationResult,
) {
    let Ok(canonical_root) = bundle_root.canonicalize() else {
        return; // Bundle root doesn't exist; not our problem here.
    };

    let refs = collect_asset_file_references(manifest);

    for (dot_path, rel_path) in &refs {
        let full_path = bundle_root.join(rel_path);
        if let Ok(canonical) = full_path.canonicalize() {
            if !canonical.starts_with(&canonical_root) {
                result.diagnostics.push(ValidationDiagnostic {
                    path: dot_path.clone(),
                    message: format!(
                        "file '{rel_path}' resolves outside the bundle sandbox (symlink escape)"
                    ),
                    severity: DiagnosticSeverity::Error,
                    category: DiagnosticCategory::PathSafety,
                });
            }
        }
    }
}

/// Check that the manifest `preset.id` matches the bundle directory basename.
///
/// When loading from a bundle directory (e.g. `~/.nexus42/presets/my-preset/`),
/// the `preset.id` field in the YAML must match the directory name. This prevents
/// identity spoofing and ensures filesystem-level consistency.
fn check_bundle_id_vs_directory(
    manifest: &PresetManifest,
    bundle_root: &Path,
    result: &mut ValidationResult,
) {
    if let Some(dir_name) = bundle_root.file_name().and_then(|n| n.to_str()) {
        let manifest_id = &manifest.preset.id;
        if manifest_id != dir_name {
            result.diagnostics.push(ValidationDiagnostic {
                path: "preset.id".to_string(),
                message: format!(
                    "preset.id '{manifest_id}' does not match bundle directory name '{dir_name}'"
                ),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::IdMismatch,
            });
        }
    }
}

/// Check that all asset file references use safe relative paths.
///
/// Reuses `loader::assert_template_file_safe` to validate each path is free of
/// `..` traversal, absolute paths, backslashes, null bytes, and control characters.
fn check_path_safety(manifest: &PresetManifest, result: &mut ValidationResult) {
    let refs = collect_asset_file_references(manifest);
    for (dot_path, rel_path) in &refs {
        if let Err(reason) = super::loader::assert_template_file_safe(rel_path) {
            // Sanitize: don't leak the full path in the error, just the relative portion.
            result.diagnostics.push(ValidationDiagnostic {
                path: dot_path.clone(),
                message: reason,
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::PathSafety,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// A4: Capability compatibility checks
// ---------------------------------------------------------------------------

/// Check capability argument compatibility.
///
/// For each enter action that invokes a capability:
/// 1. Verify the capability name exists in the registry.
/// 2. Where the capability exposes `input_schema` metadata, attempt to detect
///    obvious argument drift (unknown args, missing required args).
/// 3. If the registry lacks schema metadata, emit a "schema check skipped"
///    diagnostic so the user knows not all checks were possible.
fn check_capability_arg_compatibility(
    manifest: &PresetManifest,
    caps: &CapabilityRegistry,
    result: &mut ValidationResult,
) {
    // Also check requires_capabilities (already done by loader, but we produce
    // richer diagnostics).
    for (i, req_cap) in manifest.preset.requires_capabilities.iter().enumerate() {
        if caps.get(req_cap).is_none() {
            result.diagnostics.push(ValidationDiagnostic {
                path: format!("preset.requires_capabilities[{i}]"),
                message: format!("required capability '{req_cap}' not found in registry"),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::MissingCapability,
            });
        }
    }

    // Check enter action capabilities.
    for (i, state) in manifest.states.iter().enumerate() {
        for (j, enter) in state.enter.iter().enumerate() {
            let enter_path = format!("states[{i}].enter[{j}]");

            if let EnterAction::Capability { name, args } = enter {
                let cap_path = format!("{enter_path}.name");

                match caps.get(name) {
                    None => {
                        result.diagnostics.push(ValidationDiagnostic {
                            path: cap_path,
                            message: format!("capability '{name}' not found in registry"),
                            severity: DiagnosticSeverity::Error,
                            category: DiagnosticCategory::MissingCapability,
                        });
                    }
                    Some(cap) => {
                        // Try to parse the capability's input_schema to detect drift.
                        let schema_str = cap.input_schema();
                        if let Ok(schema_value) =
                            serde_json::from_str::<serde_json::Value>(schema_str)
                        {
                            check_args_against_schema(
                                &enter_path,
                                name,
                                args.as_ref(),
                                &schema_value,
                                result,
                            );
                        } else {
                            // Schema is not valid JSON — cannot check args.
                            result.diagnostics.push(ValidationDiagnostic {
                                path: format!("{enter_path}.args"),
                                message: format!(
                                    "schema check skipped for capability '{name}': \
                                     input_schema is not valid JSON"
                                ),
                                severity: DiagnosticSeverity::Warning,
                                category: DiagnosticCategory::SchemaCheckSkipped,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Check provided args against a capability's JSON Schema.
///
/// This is a best-effort check that looks at the top-level `properties` and
/// `required` fields of the schema. It does NOT perform full JSON Schema
/// validation (that would require a schema validation library).
fn check_args_against_schema(
    base_path: &str,
    cap_name: &str,
    args: Option<&serde_json::Value>,
    schema: &serde_json::Value,
    result: &mut ValidationResult,
) {
    let properties = schema.get("properties").and_then(|p| p.as_object());
    let required = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<HashSet<String>>()
        });

    let args_map = args.and_then(|a| a.as_object());

    // Check required args are present (top-level `required` field).
    if let Some(ref required_set) = required {
        if let Some(amap) = args_map {
            for req_arg in required_set {
                if !amap.contains_key(req_arg) {
                    result.diagnostics.push(ValidationDiagnostic {
                        path: format!("{base_path}.args"),
                        message: format!(
                            "capability '{cap_name}' requires argument '{req_arg}' \
                             which is not provided"
                        ),
                        severity: DiagnosticSeverity::Error,
                        category: DiagnosticCategory::CapabilityArgDrift,
                    });
                }
            }
        } else if !required_set.is_empty() {
            // Required args exist but no args provided at all.
            result.diagnostics.push(ValidationDiagnostic {
                path: format!("{base_path}.args"),
                message: format!(
                    "capability '{cap_name}' requires arguments {required_set:?} but none provided"
                ),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::CapabilityArgDrift,
            });
        }
    }

    // Check anyOf semantics: at least one alternative must be fully satisfied.
    if let Some(any_of) = schema.get("anyOf").and_then(|a| a.as_array()) {
        check_any_of_semantics(base_path, cap_name, args_map, any_of, result);
    }

    // Check for unknown args not in schema properties.
    if let (Some(props), Some(amap)) = (properties, args_map) {
        for key in amap.keys() {
            if !props.contains_key(key) {
                result.diagnostics.push(ValidationDiagnostic {
                    path: format!("{base_path}.args.{key}"),
                    message: format!(
                        "capability '{cap_name}' does not declare argument '{key}' \
                         in its input schema"
                    ),
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::CapabilityArgDrift,
                });
            }
        }
    }
}

/// Check `anyOf` semantics in a capability's JSON Schema.
///
/// Each `anyOf` entry may declare its own `required` array. At least one
/// alternative must have all its required fields present in the provided args.
fn check_any_of_semantics(
    base_path: &str,
    cap_name: &str,
    args_map: Option<&serde_json::Map<String, serde_json::Value>>,
    any_of: &[serde_json::Value],
    result: &mut ValidationResult,
) {
    let mut any_satisfied = false;
    for alt in any_of {
        let alt_required: HashSet<String> = alt
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if alt_required.is_empty() {
            continue;
        }
        let all_present =
            args_map.is_some_and(|amap| alt_required.iter().all(|r| amap.contains_key(r)));
        if all_present {
            any_satisfied = true;
            break;
        }
    }
    if !any_satisfied {
        let alt_labels: Vec<String> = any_of
            .iter()
            .filter_map(|alt| {
                alt.get("required").and_then(|r| {
                    let keys: Vec<String> = r
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    if keys.is_empty() {
                        None
                    } else {
                        Some(format!("{keys:?}"))
                    }
                })
            })
            .collect();
        result.diagnostics.push(ValidationDiagnostic {
            path: format!("{base_path}.args"),
            message: format!(
                "capability '{cap_name}' anyOf not satisfied: none of the \
                 required groups {alt_labels:?} are fully provided"
            ),
            severity: DiagnosticSeverity::Error,
            category: DiagnosticCategory::CapabilityArgDrift,
        });
    }
}

// ---------------------------------------------------------------------------
// A5: Run intents checks (V1.33 §5)
// ---------------------------------------------------------------------------

/// Check run intent declarations (V1.33 §5.1).
///
/// - Every `kind: creator` preset must have at least one of `work_init` or `work_continue`.
/// - `kind: system` presets may have empty `run_intents` or include `system_maintenance`.
fn check_run_intents(manifest: &PresetManifest, result: &mut ValidationResult) {
    let intents: HashSet<RunIntent> = manifest.preset.run_intents.iter().copied().collect();

    match manifest.preset.kind {
        PresetKind::Creator => {
            // R-V133P1-05: Creator presets must declare at least one run_intent.
            // Promoted from Warning to Error — the engine requires at least one
            // intent to schedule a run; surfacing at load time prevents runtime failures.
            if intents.is_empty() {
                result.diagnostics.push(ValidationDiagnostic {
                    path: "preset.run_intents".to_string(),
                    message: format!(
                        "creator preset '{}' must declare at least one run_intent (e.g. work_init, work_continue, knowledge_ingest, work_maintenance)",
                        manifest.preset.id
                    ),
                    severity: DiagnosticSeverity::Error,
                    category: DiagnosticCategory::RunIntents,
                });
            }

            // R-V133P1-05: Creator presets must not claim system-only intents.
            // Per work-experience-model §5.1, cross-claim (creator claiming
            // system_maintenance) is an Error, not a Warning.
            if intents.contains(&RunIntent::SystemMaintenance) {
                result.diagnostics.push(ValidationDiagnostic {
                    path: "preset.run_intents".to_string(),
                    message: format!(
                        "creator preset '{}' must not declare system_maintenance (system-only intent)",
                        manifest.preset.id
                    ),
                    severity: DiagnosticSeverity::Error,
                    category: DiagnosticCategory::RunIntents,
                });
            }
        }
        PresetKind::System => {
            // R-V133P1-05: System presets must not claim creator-only intents.
            // Per work-experience-model §5.1, cross-claim (system claiming
            // work_init/work_continue) is an Error.
            let creator_only_intents = [RunIntent::WorkInit, RunIntent::WorkContinue];
            let cross_claims: Vec<String> = creator_only_intents
                .iter()
                .filter(|ri| intents.contains(ri))
                .map(|ri| format!("{ri:?}"))
                .collect();
            if !cross_claims.is_empty() {
                result.diagnostics.push(ValidationDiagnostic {
                    path: "preset.run_intents".to_string(),
                    message: format!(
                        "system preset '{}' must not declare creator-only intents: {}",
                        manifest.preset.id,
                        cross_claims.join(", ")
                    ),
                    severity: DiagnosticSeverity::Error,
                    category: DiagnosticCategory::RunIntents,
                });
            }

            // System presets with run_intents should include system_maintenance.
            if !intents.is_empty() && !intents.contains(&RunIntent::SystemMaintenance) {
                result.diagnostics.push(ValidationDiagnostic {
                    path: "preset.run_intents".to_string(),
                    message: format!(
                        "system preset '{}' has run_intents but none is system_maintenance",
                        manifest.preset.id
                    ),
                    severity: DiagnosticSeverity::Warning,
                    category: DiagnosticCategory::RunIntents,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// A6: CLI args checks (V1.45 §3.3)
// ---------------------------------------------------------------------------

/// Validate `preset.cli_args` declarations (V1.45 §3.3).
///
/// Checks:
/// - Flag names match `^[a-z][a-z0-9-]*$` (kebab-case, no leading digit).
/// - No duplicate names within the same preset.
/// - `required: true` flags must not declare a `default`.
fn check_cli_args(manifest: &PresetManifest, result: &mut ValidationResult) {
    let cli_args = &manifest.preset.cli_args;
    if cli_args.is_empty() {
        return;
    }

    let mut seen: HashSet<&str> = HashSet::new();

    for arg in cli_args {
        // Name format check
        let name_valid = arg
            .name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase())
            && arg
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');

        if !name_valid {
            result.diagnostics.push(ValidationDiagnostic {
                path: "preset.cli_args".to_string(),
                message: format!(
                    "cli_arg name '{}' must be kebab-case (lowercase letters, digits, hyphens; must start with a letter)",
                    arg.name
                ),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::Structural,
            });
        }

        // Duplicate name check
        if !seen.insert(arg.name.as_str()) {
            result.diagnostics.push(ValidationDiagnostic {
                path: "preset.cli_args".to_string(),
                message: format!("duplicate cli_arg name '{}'", arg.name),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::Structural,
            });
        }

        // required + default conflict
        if arg.required && arg.default.is_some() {
            result.diagnostics.push(ValidationDiagnostic {
                path: "preset.cli_args".to_string(),
                message: format!(
                    "cli_arg '{}' is 'required: true' but also declares a 'default'; required args cannot have defaults",
                    arg.name
                ),
                severity: DiagnosticSeverity::Error,
                category: DiagnosticCategory::Structural,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityRegistry;

    fn minimal_manifest() -> PresetManifest {
        let yaml = r"
preset:
  id: tiny
  version: 1
  kind: creator
  description: minimal
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
        serde_yaml::from_str(yaml).unwrap()
    }

    // ── A2: Logical completeness ────────────────────────────────────────

    #[test]
    fn valid_preset_passes_semantic_validation() {
        let manifest = minimal_manifest();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            !result.has_errors(),
            "expected no errors: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn initial_cannot_reach_terminal() {
        // State a → b, b loops back to a, c is terminal but unreachable.
        let yaml = r"
preset:
  id: stuck
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::Reachability
                    && d.severity == DiagnosticSeverity::Error
            }),
            "expected reachability error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn no_terminal_state_at_all() {
        let yaml = r"
preset:
  id: no-term
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
    next: a
";
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::Reachability
                    && d.message.contains("no terminal state")
            }),
            "expected 'no terminal state' error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn terminal_marker_missing_on_declared_state() {
        let yaml = r"
preset:
  id: bad-marker
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::TerminalConsistency
                    && d.message.contains("not marked terminal: true")
            }),
            "expected terminal marker mismatch: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn extra_terminal_state_is_warning() {
        let yaml = r"
preset:
  id: extra-term
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
  - id: c
    terminal: true
";
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::TerminalConsistency
                    && d.severity == DiagnosticSeverity::Warning
                    && d.message.contains('c')
            }),
            "expected warning for extra terminal 'c': {:?}",
            result.diagnostics
        );
        // Should NOT have errors (b is the declared terminal and is terminal: true)
        assert!(
            !result.has_errors(),
            "expected no errors (only warnings): {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn empty_id_is_error() {
        let yaml = r"
preset:
  id: ''
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::IdMismatch
                    && d.message.contains("must not be empty")
            }),
            "expected empty id error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn invalid_id_chars_is_error() {
        let yaml = r"
preset:
  id: 'Bad Name!'
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::IdMismatch
                    && d.message.contains("invalid characters")
            }),
            "expected invalid chars error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn orphan_inner_graph_is_warning() {
        let yaml = r"
preset:
  id: orphan-test
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
  used_graph:
    nodes:
      - id: n1
        kind: acp_prompt
  orphan_graph:
    nodes:
      - id: n2
        kind: acp_prompt
";
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::OrphanInnerGraph
                    && d.severity == DiagnosticSeverity::Warning
                    && d.message.contains("orphan_graph")
            }),
            "expected orphan warning: {:?}",
            result.diagnostics
        );
        // Should not have errors (orphan is just a warning)
        assert!(
            !result.has_errors(),
            "expected no errors: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn missing_inner_graph_reference_is_error() {
        let yaml = r"
preset:
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
        name: nonexistent_graph
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
";
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::Structural
                    && d.message.contains("not defined")
                    && d.message.contains("nonexistent_graph")
            }),
            "expected missing inner graph error: {:?}",
            result.diagnostics
        );
    }

    // ── A3: Asset and path safety ───────────────────────────────────────

    #[test]
    fn missing_template_file_in_bundle_is_error() {
        let yaml = r"
preset:
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("missing-file");
        std::fs::create_dir_all(&bundle_root).unwrap();

        let result = validate_assets_in_bundle(&manifest, &bundle_root);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::MissingAsset
                    && d.message.contains("nonexistent.md")
            }),
            "expected missing asset error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn existing_template_file_passes() {
        let yaml = r"
preset:
  id: has-file
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("has-file");
        let prompts_dir = bundle_root.join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();
        std::fs::write(prompts_dir.join("judge.md"), "judge prompt").unwrap();

        let result = validate_assets_in_bundle(&manifest, &bundle_root);
        assert!(
            !result.has_errors(),
            "expected no errors: {:?}",
            result.diagnostics
        );
    }

    // ── A4: Capability compatibility ────────────────────────────────────

    #[test]
    fn unknown_requires_capabilities_is_error() {
        let yaml = r"
preset:
  id: bad-cap
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::MissingCapability
                    && d.message.contains("totally.fake.capability")
            }),
            "expected missing capability error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn unknown_enter_capability_is_error() {
        let yaml = r"
preset:
  id: bad-enter-cap
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
      - kind: capability
        name: nope.does.not.exist
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::MissingCapability
                    && d.message.contains("nope.does.not.exist")
            }),
            "expected missing capability error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn known_capability_passes() {
        let manifest = minimal_manifest();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            !result.has_errors(),
            "expected no errors for valid preset: {:?}",
            result.diagnostics
        );
    }

    // ── C4: Bundle dir id vs manifest id ────────────────────────────────

    #[test]
    fn c4_bundle_id_matches_directory() {
        let manifest = minimal_manifest(); // id = "tiny"
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("tiny"); // matches id
        std::fs::create_dir_all(&bundle_root).unwrap();
        let result = validate_assets_in_bundle(&manifest, &bundle_root);
        assert!(
            !result.has_errors(),
            "expected no errors when id matches dir: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn c4_bundle_id_mismatch_directory_is_error() {
        let manifest = minimal_manifest(); // id = "tiny"
        let tmp = tempfile::tempdir().unwrap();
        let bundle_root = tmp.path().join("other-name"); // does NOT match id
        std::fs::create_dir_all(&bundle_root).unwrap();
        let result = validate_assets_in_bundle(&manifest, &bundle_root);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::IdMismatch
                    && d.message.contains("does not match bundle directory")
            }),
            "expected id mismatch error: {:?}",
            result.diagnostics
        );
    }

    // ── W2: Path safety regression tests ────────────────────────────────

    #[test]
    fn w2_dotdot_in_template_file_is_error() {
        let yaml = r#"preset:
  id: dotdot-test
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let result = validate_path_safety(&manifest);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::PathSafety && d.message.contains("..")
            }),
            "expected path safety error for '..': {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn w2_absolute_path_in_template_file_is_error() {
        let yaml = r#"preset:
  id: abs-test
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
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let result = validate_path_safety(&manifest);
        assert!(
            result.diagnostics.iter().any(|d| {
                d.category == DiagnosticCategory::PathSafety && d.message.contains("absolute")
            }),
            "expected path safety error for absolute path: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn w2_valid_relative_path_passes_safety() {
        let manifest = minimal_manifest();
        let result = validate_path_safety(&manifest);
        assert!(
            !result.has_errors(),
            "expected no errors for valid manifest: {:?}",
            result.diagnostics
        );
    }
}

// ---------------------------------------------------------------------------
// A6: FL-E stage→preset allowlist (V1.34 §4)
// ---------------------------------------------------------------------------

pub use nexus_contracts::local::orchestration::stage_index;
/// Ordered FL-E stages — re-exported from `nexus_contracts` (single source of truth).
pub use nexus_contracts::local::orchestration::FL_E_STAGES;

/// Normative stage→preset mapping (V1.34 creator-workflow §4).
///
/// Each stage maps to one or more preset IDs that are valid for that stage.
/// The first entry is the default preset for that stage.
pub const STAGE_PRESET_ALLOWLIST: &[(&str, &[&str])] = &[
    ("intake", &["creative-brief-intake"]),
    ("research", &["research"]),
    ("produce", &["novel-writing"]),
    // R-V147P0-06 (V1.48 P0 T3): hoisted to `preset_ids` SSOT — shares the
    // single definition with the supervisor terminal guard and the
    // `persist_review_findings_for_schedule` findings hook.
    (
        "review",
        &[crate::preset_ids::NOVEL_CHAPTER_REVIEW_PRESET_ID],
    ),
    // R-FL-E-06: persist stage default is kb-extract (via queue)
    ("persist", &["kb-extract"]),
];

/// Returns the default preset ID for a given FL-E stage.
///
/// Returns `None` if the stage is not a valid FL-E stage.
#[must_use]
pub fn default_preset_for_stage(stage: &str) -> Option<&'static str> {
    STAGE_PRESET_ALLOWLIST
        .iter()
        .find(|(s, _)| *s == stage)
        .map(|(_, presets)| presets[0])
}

/// Reverse mapping: given a preset ID, return the FL-E stage it belongs to.
///
/// Used by the generic `creator run <preset_id>` runner (V1.45 §4) to
/// determine whether a preset is an FL-E stage-advance preset and, if so,
/// which stage to advance.
///
/// Returns `None` for preset IDs that are not in any FL-E stage allowlist
/// (e.g. `novel-brainstorm`, `novel-review-master`, audit presets).
#[must_use]
pub fn stage_for_preset(preset_id: &str) -> Option<&'static str> {
    STAGE_PRESET_ALLOWLIST
        .iter()
        .find(|(_, presets)| presets.contains(&preset_id))
        .map(|(stage, _)| *stage)
}

/// Returns all allowed preset IDs for a given FL-E stage.
///
/// Returns `None` if the stage is not a valid FL-E stage.
#[must_use]
pub fn allowed_presets_for_stage(stage: &str) -> Option<&'static [&'static str]> {
    STAGE_PRESET_ALLOWLIST
        .iter()
        .find(|(s, _)| *s == stage)
        .map(|(_, presets)| *presets)
}

/// Validates that a preset ID is in the allowlist for a given FL-E stage.
///
/// Returns `Ok(())` if the preset is allowed for the stage, or a descriptive
/// error string if not.
///
/// # Errors
///
/// Returns `Err(String)` with a descriptive message if:
/// - The stage is not a valid FL-E stage
/// - The preset is not in the allowlist for that stage
pub fn validate_preset_for_stage(stage: &str, preset_id: &str) -> std::result::Result<(), String> {
    let allowed = allowed_presets_for_stage(stage).ok_or_else(|| {
        format!(
            "Unknown FL-E stage '{stage}'. Valid stages: {}",
            FL_E_STAGES.join(", ")
        )
    })?;

    if allowed.contains(&preset_id) {
        Ok(())
    } else {
        Err(format!(
            "Preset '{preset_id}' is not in the allowlist for stage '{stage}'. \
             Allowed presets: {}",
            allowed.join(", ")
        ))
    }
}

#[cfg(test)]
mod stage_tests {
    use super::*;

    #[test]
    fn default_preset_for_known_stages() {
        assert_eq!(
            default_preset_for_stage("intake"),
            Some("creative-brief-intake")
        );
        assert_eq!(default_preset_for_stage("research"), Some("research"));
        assert_eq!(default_preset_for_stage("produce"), Some("novel-writing"));
        assert_eq!(
            default_preset_for_stage("review"),
            Some("novel-chapter-review")
        );
        assert_eq!(default_preset_for_stage("persist"), Some("kb-extract"));
    }

    #[test]
    fn default_preset_for_unknown_stage() {
        assert_eq!(default_preset_for_stage("unknown"), None);
    }

    #[test]
    fn validate_known_preset_for_stage() {
        assert!(validate_preset_for_stage("intake", "creative-brief-intake").is_ok());
        assert!(validate_preset_for_stage("produce", "novel-writing").is_ok());
    }

    #[test]
    fn validate_wrong_preset_for_stage() {
        assert!(validate_preset_for_stage("intake", "novel-writing").is_err());
        assert!(validate_preset_for_stage("produce", "creative-brief-intake").is_err());
    }

    #[test]
    fn validate_unknown_stage() {
        assert!(validate_preset_for_stage("unknown", "any-preset").is_err());
    }

    #[test]
    fn stage_index_order() {
        assert_eq!(stage_index("intake"), Some(0));
        assert_eq!(stage_index("research"), Some(1));
        assert_eq!(stage_index("produce"), Some(2));
        assert_eq!(stage_index("review"), Some(3));
        assert_eq!(stage_index("persist"), Some(4));
    }

    #[test]
    fn stage_index_unknown() {
        assert_eq!(stage_index("unknown"), None);
    }

    #[test]
    fn persist_allowlist_accepts_kb_extract_only() {
        // BL-13: memory-review was listed but no preset exists; removed.
        assert!(
            validate_preset_for_stage("persist", "kb-extract").is_ok(),
            "persist should accept kb-extract"
        );
        assert!(
            validate_preset_for_stage("persist", "memory-review").is_err(),
            "persist should NOT accept memory-review (no such preset)"
        );
        // Default should still be kb-extract (first entry)
        assert_eq!(default_preset_for_stage("persist"), Some("kb-extract"));
    }

    // ── V1.52 T-B P1: merge node integrity tests ───────────────────────────

    #[test]
    fn merge_node_valid_all_with_2_incoming() {
        let yaml = r#"
preset:
  id: merge-valid
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: done
states:
  - id: a
    enter: []
    exit_when: { kind: llm_judge }
    next:
      - label: x
        target: merged
  - id: b
    enter: []
    exit_when: { kind: llm_judge }
    next:
      - label: y
        target: merged
  - id: merged
    merge:
      kind: all
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            !result
                .errors()
                .any(|d| d.category == DiagnosticCategory::MergeIntegrity),
            "valid merge node should not produce errors: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn merge_node_too_few_incoming_errors() {
        let yaml = r#"
preset:
  id: merge-few
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: done
states:
  - id: a
    enter: []
    exit_when: { kind: llm_judge }
    next:
      - label: x
        target: merged
  - id: merged
    merge:
      kind: all
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.errors().any(|d| {
                d.category == DiagnosticCategory::MergeIntegrity && d.message.contains("2 incoming")
            }),
            "merge node with <2 incoming edges should error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn quorum_n_exceeds_m_errors() {
        let yaml = r#"
preset:
  id: merge-bad-quorum
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: done
states:
  - id: a
    enter: []
    exit_when: { kind: llm_judge }
    next:
      - label: x
        target: merged
      - label: y
        target: merged
      - label: z
        target: merged
  - id: merged
    merge:
      kind: quorum
      n: 4
      m: 3
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.errors().any(|d| {
                d.category == DiagnosticCategory::MergeIntegrity
                    && d.message.contains("must not exceed")
            }),
            "quorum with n > m should error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn quorum_n_zero_errors() {
        let yaml = r#"
preset:
  id: merge-zero-n
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: done
states:
  - id: a
    enter: []
    exit_when: { kind: llm_judge }
    next:
      - label: x
        target: merged
      - label: y
        target: merged
  - id: merged
    merge:
      kind: quorum
      n: 0
      m: 2
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.errors().any(|d| {
                d.category == DiagnosticCategory::MergeIntegrity
                    && d.message.contains("n must be >=")
            }),
            "quorum with n<1 should error: {:?}",
            result.diagnostics
        );
    }

    // ── W-QC3-1: cross-state label duplicate check ────────────────────

    #[test]
    fn cross_state_label_duplicate_errors() {
        let yaml = r#"
preset:
  id: cross-label-dup
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: done
states:
  - id: a
    enter: []
    exit_when: { kind: llm_judge }
    next:
      - label: foo
        target: merged
  - id: b
    enter: []
    exit_when: { kind: llm_judge }
    next:
      - label: foo
        target: merged
  - id: merged
    merge:
      kind: all
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.errors().any(|d| {
                d.category == DiagnosticCategory::DuplicateLabel
                    && d.message.contains("state 'b'")
                    && d.message.contains("state 'a'")
            }),
            "cross-state duplicate label should produce DuplicateLabel error: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn quorum_m_mismatch_errors() {
        let yaml = r#"
preset:
  id: merge-m-mismatch
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: done
states:
  - id: a
    enter: []
    exit_when: { kind: llm_judge }
    next:
      - label: x
        target: merged
      - label: y
        target: merged
      - label: z
        target: merged
  - id: merged
    merge:
      kind: quorum
      n: 1
      m: 5
    exit_when: { kind: manual }
    next: done
  - id: done
    terminal: true
"#;
        let manifest: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let caps = CapabilityRegistry::with_builtins();
        let result = validate_preset_semantic(&manifest, &caps);
        assert!(
            result.errors().any(|d| {
                d.category == DiagnosticCategory::MergeIntegrity
                    && d.message.contains("does not match actual incoming")
            }),
            "quorum with m != actual incoming should error: {:?}",
            result.diagnostics
        );
    }
}
