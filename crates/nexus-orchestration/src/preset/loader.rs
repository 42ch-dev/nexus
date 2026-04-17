//! Preset loader + validation.
//!
//! Parses `preset.yaml` → `PresetManifest` → validates per §7.6 → produces
//! a `LoadedPreset` with outer/inner `graph-flow::Graph` instances.
//!
//! Design: `orchestration-engine-v1.md` §8.1.

use crate::capability::CapabilityRegistry;
use crate::preset::manifest::{ExitWhen, InnerGraph, NextTarget, PresetManifest};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// LoadedPreset
// ---------------------------------------------------------------------------

/// A fully validated preset ready for execution.
///
/// Design: `orchestration-engine-v1.md` §8.1.
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
            .finish()
    }
}

// ---------------------------------------------------------------------------
// PresetLoadError
// ---------------------------------------------------------------------------

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
}

impl PresetLoadError {
    /// Borrow the list of validation problems (if this is a validation error).
    pub fn problems(&self) -> &[ValidationProblem] {
        match self {
            PresetLoadError::Validation { problems, .. } => problems,
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
/// Validates all §7.6 rules. Does NOT validate template file paths (those
/// require a filesystem root; use [`load_preset`] for that).
///
/// `source_hash` is blake3 over the YAML string.
pub fn load_preset_from_str(
    yaml: &str,
    caps: &CapabilityRegistry,
) -> Result<LoadedPreset, PresetLoadError> {
    // 1. Parse YAML.
    let manifest: PresetManifest = serde_yaml::from_str(yaml)?;

    // 2. Validate.
    let problems = validate_manifest(&manifest, caps);
    if !problems.is_empty() {
        return Err(PresetLoadError::Validation {
            len: problems.len(),
            problems,
        });
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
        manifest,
    })
}

/// Load a preset from a bundle directory on disk.
///
/// Reads `preset.yaml` from the bundle root and delegates to
/// [`load_preset_from_str`].
///
/// Future: also reads prompt templates and validates template_file paths.
pub fn load_preset(
    _bundle_root: &Path,
    _caps: &CapabilityRegistry,
) -> Result<LoadedPreset, PresetLoadError> {
    // T6 (embedded presets) will implement the real file-system loading.
    Err(PresetLoadError::Validation {
        len: 1,
        problems: vec![ValidationProblem {
            path: String::new(),
            error: "load_preset from directory not yet implemented (WS3 T6)".to_string(),
        }],
    })
}

// ---------------------------------------------------------------------------
// Validation (§7.6)
// ---------------------------------------------------------------------------

/// Run all §7.6 validation rules against a parsed manifest.
///
/// Returns a list of problems (empty = valid).
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
                path: format!("preset.requires_capabilities[{}]", i),
                error: format!("required capability not found in registry: '{}'", req_cap),
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
        let state_path = format!("states[{}]", i);

        // Check next state reference
        if let Some(ref next) = state.next {
            match next {
                NextTarget::Linear(target_id) => {
                    if !state_ids.contains(target_id.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{}.next", state_path),
                            error: format!("unknown state: '{}'", target_id),
                        });
                    }
                }
                NextTarget::Conditional(_) => {
                    problems.push(ValidationProblem {
                        path: format!("{}.next", state_path),
                        error: "conditional next is not yet supported in V1.4 (ConditionalNotYetSupported)"
                            .to_string(),
                    });
                }
            }
        }

        // Check that the terminal state has no next
        if state.terminal && state.next.is_some() {
            problems.push(ValidationProblem {
                path: format!("{}.terminal", state_path),
                error: "terminal state must not have a 'next' field".to_string(),
            });
        }

        // Check enter actions
        for (j, enter) in state.enter.iter().enumerate() {
            let enter_path = format!("{}.enter[{}]", state_path, j);
            match enter {
                crate::preset::manifest::EnterAction::Capability { name, .. } => {
                    if caps.get(name).is_none() {
                        problems.push(ValidationProblem {
                            path: format!("{}.name", enter_path),
                            error: format!("unknown capability: '{}'", name),
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
                            path: format!("{}.name", enter_path),
                            error: format!("unknown inner_graph: '{}'", name),
                        });
                    }
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
                    path: format!("{}.exit_when.judge_capability", state_path),
                    error: format!("unknown capability: '{}'", cap_name),
                });
            }
        }
    }

    // Validate inner graphs
    if let Some(ref inner_graphs) = manifest.inner_graphs {
        for (name, ig) in inner_graphs.iter() {
            let ig_path = format!("inner_graphs.{}", name);

            // Cycle detection on depends_on
            let cycle_path = ig_path.clone();
            if let Some(cycle) = detect_cycle(ig) {
                problems.push(ValidationProblem {
                    path: cycle_path,
                    error: format!("cycle detected: {}", cycle),
                });
            }

            // Check depends_on references
            let node_ids: HashSet<&str> = ig.nodes.iter().map(|n| n.id.as_str()).collect();
            for (k, node) in ig.nodes.iter().enumerate() {
                for dep in &node.depends_on {
                    if !node_ids.contains(dep.as_str()) {
                        problems.push(ValidationProblem {
                            path: format!("{}.nodes[{}].depends_on", ig_path, k),
                            error: format!("unknown node: '{}'", dep),
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
                        path: format!("{}.output_binding", ig_path),
                        error: format!("output_binding references unknown node: '{}'", node_id),
                    });
                }
            }
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
                let mut parts: Vec<String> =
                    path[cycle_start..].iter().map(|s| s.to_string()).collect();
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

// ---------------------------------------------------------------------------
// Graph building per §8.2 mapping table
// ---------------------------------------------------------------------------

/// Build the outer state-machine graph per §8.2.
///
/// Each `states[].id` → a composite `Task` that encodes the enter actions,
/// exit_when condition, and terminal semantics.
fn build_outer_graph(manifest: &PresetManifest) -> graph_flow::Graph {
    use crate::tasks::StateCompositeTask;

    let graph = graph_flow::Graph::new(&manifest.preset.id);

    for state in &manifest.states {
        let task = StateCompositeTask::from_manifest(state);
        graph.add_task(std::sync::Arc::new(task));
    }

    // Wire edges from state.next (linear only; conditional already rejected by validation).
    for state in &manifest.states {
        if let Some(NextTarget::Linear(ref next_id)) = state.next {
            graph.add_edge(&state.id, next_id);
        }
    }

    graph
}

/// Build the outer graph with engine + inner graph references wired into
/// composite tasks (for `start_session_with_preset`).
pub fn build_wired_outer_graph(
    loaded: &LoadedPreset,
    engine: Arc<dyn crate::engine::OrchestrationEngine>,
    caps: Arc<CapabilityRegistry>,
) -> graph_flow::Graph {
    use crate::tasks::StateCompositeTask;

    let graph = graph_flow::Graph::new(&loaded.id);

    for state in &loaded.manifest.states {
        let task = StateCompositeTask::from_manifest(state)
            .with_engine(engine.clone())
            .with_inner_graphs(loaded.inner_graphs.clone())
            .with_output_bindings(loaded.output_bindings.clone())
            .with_registry(caps.clone());
        graph.add_task(std::sync::Arc::new(task));
    }

    // Wire edges.
    for state in &loaded.manifest.states {
        if let Some(NextTarget::Linear(ref next_id)) = state.next {
            graph.add_edge(&state.id, next_id);
        }
    }

    graph
}

/// Extract output bindings from the manifest's inner_graphs.
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
fn build_inner_graphs(manifest: &PresetManifest) -> HashMap<String, Arc<graph_flow::Graph>> {
    use crate::tasks::InnerGraphNodeTask;

    let mut result = HashMap::new();

    if let Some(ref inner_graphs) = manifest.inner_graphs {
        for (name, ig) in inner_graphs {
            let graph = graph_flow::Graph::new(name);

            for node in &ig.nodes {
                let task = InnerGraphNodeTask::new(&node.id);
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
        r#"
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
"#
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
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
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
        let err = load_preset_from_str(yaml, &caps).unwrap_err();
        let problems = err.problems();
        assert!(
            problems
                .iter()
                .any(|p| p.error.contains("ConditionalNotYetSupported")),
            "expected 'ConditionalNotYetSupported' problem: {problems:?}"
        );
    }

    #[test]
    fn reject_unknown_inner_graph_reference() {
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps).unwrap();
        assert!(loaded.inner_graphs.contains_key("my_graph"));
        let ig = &loaded.inner_graphs["my_graph"];
        assert!(ig.get_task("n1").is_some());
        assert!(ig.get_task("n2").is_some());
    }

    #[test]
    fn reject_unknown_depends_on_in_inner_graph() {
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
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
        let yaml2 = r#"
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
"#;
        let h2 = load_preset_from_str(yaml2, &caps).unwrap().source_hash;
        assert_ne!(h1, h2);
    }

    #[test]
    fn reject_unknown_requires_capabilities() {
        let yaml = r#"
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
"#;
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
        let yaml = r#"
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
"#;
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps);
        assert!(
            loaded.is_ok(),
            "expected valid preset with known requires_capabilities: {loaded:?}"
        );
    }
}
