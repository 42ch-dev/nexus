//! Preset manifest types — hand-written, per `schemas-boundary-v1.md` §3.
//!
//! These types represent the YAML schema for `preset.yaml` as defined in
//! `orchestration-engine-v1.md` §7.2 and §7.5.
//!
//! **NOT** in `schemas/` — this is a local type; `nexus-platform` never
//! observes it over any wire channel.
//!
//! ## Roles and `recommended_models` (`WS-E` §7)
//!
//! Presets define role-based agent configurations:
//! - `roles`: list of `PresetRoleDefinition` with `recommended_models`
//! - `GraphNode.agent`: optional role `ID` reference
//!
//! Backward compatible: presets without `roles` operate in single-agent mode.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Top-level manifest
// ---------------------------------------------------------------------------

/// Root wrapper for a preset YAML file.
///
/// ```yaml
/// preset:
///   id: novel-writing
///   version: 1
///   ...
/// states:
///   - ...
/// roles:
///   - id: writer
///     ...
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetManifest {
    /// Preset header metadata.
    pub preset: PresetHeader,
    /// Ordered list of state definitions.
    pub states: Vec<StateDefinition>,
    /// Optional inner graph definitions referenced by `enter.kind = inner_graph`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_graphs: Option<std::collections::BTreeMap<String, InnerGraph>>,
    /// Optional signal bindings (external events).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<SignalBinding>,
    /// Optional role definitions for multi-agent presets (WS-E §7).
    /// Each role defines `recommended_models` and a `system_prompt_file`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<PresetRoleDefinition>,
}

// ---------------------------------------------------------------------------
// PresetHeader
// ---------------------------------------------------------------------------

/// Metadata and configuration for the preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetHeader {
    /// Preset identifier (must match directory name; `/^[a-z][a-z0-9._-]*$/`).
    pub id: String,
    /// Schema version (>= 1; bumped on breaking changes to this preset).
    pub version: u32,
    /// Preset kind: `creator` (user-facing) or `system` (internal).
    pub kind: PresetKind,
    /// Human-readable description (<= 240 chars).
    pub description: String,
    /// Capabilities this preset requires; loader rejects if any are missing.
    #[serde(default)]
    pub requires_capabilities: Vec<String>,
    /// The ID of the initial state (must match a `states[].id`).
    pub initial: String,
    /// The ID of the terminal state (must match a `states[].id`).
    pub terminal: String,
    /// Optional author name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Optional homepage URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Optional license identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Optional initial action for schedule creation (`WS7` §7).
    /// Controls how `core_context` v0 is seeded when a schedule is created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_action: Option<InitialAction>,
}

/// Preset kind discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    /// User-facing creator preset.
    Creator,
    /// Internal system preset (e.g. `_system.maintenance`).
    System,
}

// ---------------------------------------------------------------------------
// StateDefinition
// ---------------------------------------------------------------------------

/// A single state in the outer state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDefinition {
    /// Unique state identifier within this preset.
    pub id: String,
    /// Optional human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Actions to execute when entering this state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enter: Vec<EnterAction>,
    /// Condition that must be satisfied before transitioning to `next`.
    /// May be absent for terminal states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_when: Option<ExitWhen>,
    /// Next state ID (linear) or conditional form.
    ///
    /// If `None`, this is a terminal state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<NextTarget>,
    /// Whether this state is terminal (no outgoing transitions).
    #[serde(default, skip_serializing_if = "is_false")]
    pub terminal: bool,
    /// Optional context update hook that fires on state exit (WS7 §7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_update: Option<ContextUpdateHook>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !b
}

// ---------------------------------------------------------------------------
// EnterAction
// ---------------------------------------------------------------------------

/// An action to execute when entering a state.
///
/// Uses `#[serde(tag = "kind")]` for tag-based YAML variants:
/// ```yaml
/// - kind: capability
///   name: creator.inject_prompt
///   args: { ... }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EnterAction {
    /// Invoke a registered capability by name.
    #[serde(rename = "capability")]
    Capability {
        /// Dot-separated capability name, e.g. `creator.inject_prompt`.
        name: String,
        /// Capability-specific arguments.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<serde_json::Value>,
    },
    /// Launch an inner graph (child session over a named inner graph).
    #[serde(rename = "inner_graph")]
    InnerGraph {
        /// Name of the inner graph (must match `inner_graphs.<name>`).
        name: String,
    },
}

// ---------------------------------------------------------------------------
// ExitWhen
// ---------------------------------------------------------------------------

/// Condition that must be satisfied before transitioning.
///
/// Uses `#[serde(tag = "kind")]` for tag-based YAML variants:
/// ```yaml
/// exit_when:
///   kind: manual
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ExitWhen {
    /// LLM-backed judge evaluates a go/nogo prompt.
    #[serde(rename = "llm_judge")]
    LlmJudge {
        /// Path to the judge prompt template.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        template_file: Option<String>,
        /// Which judge capability to call (default: `judge.llm`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        judge_capability: Option<String>,
        /// Minimum interval (ISO-8601 duration) between re-evaluations.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_interval: Option<String>,
    },
    /// Pure function evaluation over context.
    #[serde(rename = "rule")]
    Rule,
    /// Inner graph has completed execution.
    #[serde(rename = "graph_complete")]
    GraphComplete,
    /// User-driven advance (manual).
    #[serde(rename = "manual")]
    Manual,
    /// Timer-based wait.
    #[serde(rename = "timer")]
    Timer {
        /// ISO-8601 duration to wait.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// NextTarget (linear or conditional)
// ---------------------------------------------------------------------------

/// Transition target — linear ID or conditional form.
///
/// In YAML, this can be either:
/// ```yaml
/// next: brainstorming          # linear
/// ```
/// or:
/// ```yaml
/// next:
///   kind: conditional
///   rules: [...]
///   default: outlining
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NextTarget {
    /// Linear transition to a single state ID.
    Linear(String),
    /// Conditional transition (V1.4 returns error; not yet implemented).
    Conditional(NextConditional),
}

/// Conditional next form — V1.4 does NOT implement this; loader returns error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextConditional {
    /// Must be `"conditional"`.
    pub kind: String,
    /// Ordered list of conditional rules.
    #[serde(default)]
    pub rules: Vec<ConditionalRule>,
    /// Default target state if no rule matches.
    pub default: String,
}

/// A single conditional rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionalRule {
    /// Handlebars-style when-condition expression.
    pub when: String,
    /// Target state ID if the condition matches.
    pub to: String,
}

// ---------------------------------------------------------------------------
// InnerGraph
// ---------------------------------------------------------------------------

/// Definition of an inner graph (DAG of prompt/tool nodes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerGraph {
    /// Ordered list of graph nodes.
    pub nodes: Vec<GraphNode>,
    /// Which node's output is exported as the state's output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_binding: Option<String>,
}

/// A single node in an inner graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Unique node ID within this inner graph.
    pub id: String,
    /// Node kind: `acp_prompt` (and others in future).
    pub kind: GraphNodeKind,
    /// Ordered list of node IDs this node depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Template file path for prompt templates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_file: Option<String>,
    /// Tool policy for ACP prompt nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<String>,
    /// Optional role ID reference for multi-agent presets (WS-E §7).
    /// Must match a role ID in `roles[]` if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// Kind of inner-graph node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    /// Send a prompt to the ACP agent.
    AcpPrompt,
}

// ---------------------------------------------------------------------------
// InitialAction (WS7 §7)
// ---------------------------------------------------------------------------

/// What action to take when a schedule starts using this preset.
///
/// Declared at `preset.initial_action` level in the YAML manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InitialAction {
    /// Use the seed text directly as `core_context` v0.
    SeedDirect,
    /// Expand the seed using a registered capability (V1.5+).
    SeedExpansion {
        /// Capability to call for expansion (e.g. `context.summarize`).
        capability: String,
        /// Handlebars template file for the expansion prompt.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        template_file: Option<String>,
        /// Expected payload kind after expansion.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload_kind: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// ContextUpdateHook (WS7 §7)
// ---------------------------------------------------------------------------

/// A hook that fires on state exit to update the schedule's `core_context`.
///
/// Declared per-state as `states[].context_update` in the YAML manifest.
/// Only `Append` and `StructMerge` operations are allowed; `Replace` is
/// rejected during validation (spec §6.2 — preset hooks are strictly additive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextUpdateHook {
    /// The edit operation to apply.
    /// Only `append` and `struct_merge` kinds are valid for preset hooks.
    pub op: ContextUpdateOp,
    /// Handlebars template file to render the edit content.
    pub template_file: String,
}

/// Edit operation shape for `context_update` hooks.
///
/// A simplified subset of [`nexus_contracts::local::schedule::EditOp`] that
/// is used at the YAML parsing level (before converting to the full `EditOp`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextUpdateOp {
    /// Append text to existing content.
    Append {
        /// Body content (empty by default; filled at runtime from template).
        #[serde(default)]
        body: String,
    },
    /// JSON-merge into struct payload.
    StructMerge {
        /// Patch JSON (empty by default; filled at runtime from template).
        #[serde(default)]
        patch: serde_json::Value,
    },
    /// Replace is NOT allowed for preset hooks — will be rejected by the loader.
    Replace {
        #[serde(default)]
        body: String,
    },
    /// `StructRemove` is NOT allowed for preset hooks — will be rejected by the loader.
    StructRemove {
        #[serde(default)]
        path: String,
    },
    /// V1.5+ only. Invoke `context.summarize` capability to produce an
    /// LLM-driven summary of the current `core_context`.
    LlmSummarize {
        /// Capability name to invoke (e.g. `context.summarize`).
        capability: String,
    },
}

// ---------------------------------------------------------------------------
// SignalBinding
// ---------------------------------------------------------------------------

/// A signal that can externally push the state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalBinding {
    /// Signal name.
    pub name: String,
    /// What to do when the signal is received.
    pub on_receive: SignalAction,
}

/// Action to take when a signal is received.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalAction {
    /// The action kind.
    pub action: SignalActionKind,
    /// Target state ID (for `force_transition`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Signal action kinds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalActionKind {
    /// Pause execution.
    Pause,
    /// Force transition to a target state.
    ForceTransition,
}

// ---------------------------------------------------------------------------
// PresetRoleDefinition (WS-E §7)
// ---------------------------------------------------------------------------

/// A role definition for multi-agent presets (`WS-E` §7).
///
/// Defines a named agent role with:
/// - A system prompt template (via `system_prompt_file`)
/// - Recommended agent:model pairs (ordered list, first = default)
///
/// At runtime, the daemon resolves each role to an `acp_agent_id` + `model`
/// using the priority resolution order (`CLI` > user config > `recommended_models`).
///
/// ```yaml
/// roles:
///   - id: writer
///     description: "Primary content writer"
///     system_prompt_file: prompts/writer-system.md
///     recommended_models:
///       - "claude-acp:claude-sonnet-4-20250514"
///       - "gemini:gemini-2.5-pro"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetRoleDefinition {
    /// Unique role ID within this preset.
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Path to system prompt template (relative to preset bundle root).
    pub system_prompt_file: String,
    /// Ordered list of `acp_agent_id:model_name` pairs.
    /// First entry is the default; subsequent entries are fallbacks.
    /// Format validated by loader: must contain exactly one colon.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_models: Vec<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Full preset YAML with `inner_graphs` and signals (shared across tests).
    static FULL_PRESET_YAML: &str = r#"
preset:
  id: novel-writing
  version: 1
  kind: creator
  description: "A novel writing workflow"
  requires_capabilities:
    - creator.inject_prompt
    - acp.prompt
    - judge.llm
  initial: gathering
  terminal: done
  author: "42ch"
  homepage: "https://example.com"
  license: "MIT"
states:
  - id: gathering
    description: "Collect inspiration"
    enter:
      - kind: capability
        name: creator.inject_prompt
        args:
          prompt_file: prompts/gathering.md
          vars:
            topic: "{{preset.input.topic}}"
    exit_when:
      kind: llm_judge
      template_file: prompts/gathering-exit.md
      judge_capability: judge.llm
      min_interval: "PT6H"
    next: brainstorming
  - id: brainstorming
    enter:
      - kind: inner_graph
        name: brainstorm_graph
    exit_when:
      kind: graph_complete
    next: outlining
  - id: outlining
    enter:
      - kind: capability
        name: creator.inject_prompt
        args:
          prompt_file: prompts/outlining.md
    exit_when:
      kind: manual
    next: drafting
  - id: drafting
    enter:
      - kind: inner_graph
        name: drafting_graph
    exit_when:
      kind: graph_complete
    next: done
  - id: done
    terminal: true
inner_graphs:
  brainstorm_graph:
    nodes:
      - id: diverge
        kind: acp_prompt
        template_file: prompts/brainstorm-diverge.md
        tool_policy: auto_grant_read_only
      - id: cluster
        kind: acp_prompt
        depends_on: [diverge]
        template_file: prompts/brainstorm-cluster.md
      - id: select
        kind: acp_prompt
        depends_on: [cluster]
        template_file: prompts/brainstorm-select.md
    output_binding: select.text
  drafting_graph:
    nodes:
      - id: draft_intro
        kind: acp_prompt
        template_file: prompts/draft-intro.md
      - id: draft_body
        kind: acp_prompt
        depends_on: [draft_intro]
        template_file: prompts/draft-body.md
    output_binding: draft_body.text
signals:
  - name: user_paused
    on_receive:
      action: pause
  - name: deadline_reached
    on_receive:
      action: force_transition
      target: done
"#;

    #[test]
    fn parse_minimal_creator_preset() {
        let yaml = r"
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
";
        let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.preset.id, "tiny");
        assert_eq!(p.preset.version, 1);
        assert_eq!(p.preset.kind, PresetKind::Creator);
        assert_eq!(p.states.len(), 2);
        assert_eq!(p.states[0].id, "a");
        assert!(p.states[1].terminal);
    }

    #[test]
    fn parse_full_preset_with_inner_graphs_and_signals() {
        let yaml = FULL_PRESET_YAML;
        assert_parsed_full_preset(yaml);
    }

    /// Shared helper: parse the full-preset YAML and assert key invariants.
    fn assert_parsed_full_preset(yaml: &str) {
        let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.preset.id, "novel-writing");
        assert_eq!(p.preset.version, 1);
        assert_eq!(p.preset.requires_capabilities.len(), 3);
        assert_eq!(p.states.len(), 5);
        assert_eq!(p.states[0].enter.len(), 1);
        assert_eq!(
            p.states[0].next.as_ref().unwrap(),
            &NextTarget::Linear("brainstorming".into())
        );
        assert!(p.inner_graphs.is_some());
        let ig = p.inner_graphs.as_ref().unwrap();
        assert_eq!(ig.len(), 2);
        assert_eq!(ig["brainstorm_graph"].nodes.len(), 3);
        assert_eq!(
            ig["brainstorm_graph"].output_binding.as_deref(),
            Some("select.text")
        );
        assert_eq!(p.signals.len(), 2);
    }

    #[test]
    fn unknown_exit_when_kind_fails_with_clear_error() {
        let yaml = r"
preset:
  id: bad
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
      kind: quantum_teleport
    next: b
  - id: b
    terminal: true
";
        let err = serde_yaml::from_str::<PresetManifest>(yaml);
        assert!(
            err.is_err(),
            "expected serde error for unknown exit_when.kind"
        );
        let msg = format!("{:#}", err.unwrap_err());
        assert!(
            msg.contains("quantum_teleport"),
            "error message should mention the unknown variant: {msg}"
        );
    }

    #[test]
    fn unknown_enter_kind_fails_with_clear_error() {
        let yaml = r"
preset:
  id: bad
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: launch_rocket
        name: falcon-9
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";
        let err = serde_yaml::from_str::<PresetManifest>(yaml);
        assert!(err.is_err(), "expected serde error for unknown enter.kind");
        let msg = format!("{:#}", err.unwrap_err());
        assert!(
            msg.contains("launch_rocket"),
            "error message should mention the unknown variant: {msg}"
        );
    }

    #[test]
    fn system_preset_kind_parses() {
        let yaml = r"
preset:
  id: _system.maintenance
  version: 1
  kind: system
  description: internal
  requires_capabilities: []
  initial: sync
  terminal: end
states:
  - id: sync
    enter: []
    exit_when: { kind: rule }
    next: end
  - id: end
    terminal: true
";
        let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.preset.kind, PresetKind::System);
    }

    #[test]
    fn conditional_next_target_parses_but_not_implemented() {
        let yaml = r#"
preset:
  id: cond
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
        - when: "{{state.a.output | length > 2000}}"
          to: c
      default: b
  - id: b
    enter: []
    exit_when: { kind: manual }
    next: c
  - id: c
    terminal: true
"#;
        let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        match &p.states[0].next {
            Some(NextTarget::Conditional(c)) => {
                assert_eq!(c.kind, "conditional");
                assert_eq!(c.rules.len(), 1);
                assert_eq!(c.default, "b");
            }
            _ => panic!("expected conditional next target"),
        }
    }

    // ── WS-E T6: Roles and recommended_models ──────────────────────────────

    #[test]
    fn parse_preset_with_roles() {
        let yaml = r#"
preset:
  id: multi-agent-demo
  version: 1
  kind: creator
  description: "Multi-agent workflow"
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
roles:
  - id: writer
    description: "Primary content writer"
    system_prompt_file: prompts/writer-system.md
    recommended_models:
      - "claude-acp:claude-sonnet-4-20250514"
      - "gemini:gemini-2.5-pro"
  - id: reviewer
    description: "Content reviewer"
    system_prompt_file: prompts/reviewer-system.md
    recommended_models:
      - "codex-acp:o3"
"#;
        let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.roles.len(), 2);
        assert_eq!(p.roles[0].id, "writer");
        assert_eq!(p.roles[0].description, "Primary content writer");
        assert_eq!(p.roles[0].system_prompt_file, "prompts/writer-system.md");
        assert_eq!(p.roles[0].recommended_models.len(), 2);
        assert_eq!(
            p.roles[0].recommended_models[0],
            "claude-acp:claude-sonnet-4-20250514"
        );
        assert_eq!(p.roles[1].id, "reviewer");
        assert_eq!(p.roles[1].recommended_models.len(), 1);
    }

    #[test]
    fn parse_graph_node_with_agent_field() {
        let yaml = r#"
preset:
  id: agent-node-test
  version: 1
  kind: creator
  description: "Test agent field in graph nodes"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: work_graph
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
roles:
  - id: writer
    description: "Writer"
    system_prompt_file: prompts/writer.md
    recommended_models:
      - "claude-acp:claude-sonnet-4-20250514"
inner_graphs:
  work_graph:
    nodes:
      - id: draft
        kind: acp_prompt
        agent: writer
        template_file: prompts/draft.md
      - id: review
        kind: acp_prompt
        depends_on: [draft]
        template_file: prompts/review.md
    output_binding: review.text
"#;
        let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        let ig = p.inner_graphs.as_ref().unwrap();
        let nodes = &ig["work_graph"].nodes;
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].agent.as_deref(), Some("writer"));
        assert!(nodes[1].agent.is_none());
    }

    #[test]
    fn preset_without_roles_is_backward_compatible() {
        // Existing presets without roles should still parse correctly.
        let yaml = r#"
preset:
  id: single-agent
  version: 1
  kind: creator
  description: "Single-agent preset"
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
        let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        assert!(p.roles.is_empty());
    }

    #[test]
    fn role_without_recommended_models_parses() {
        // recommended_models is optional (can be empty).
        // Loader will reject empty recommended_models during validation.
        let yaml = r#"
preset:
  id: empty-roles
  version: 1
  kind: creator
  description: "Preset with empty recommended_models"
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
roles:
  - id: writer
    description: "Writer"
    system_prompt_file: prompts/writer.md
"#;
        let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.roles.len(), 1);
        assert!(p.roles[0].recommended_models.is_empty());
    }

    #[test]
    fn roundtrip_serialize_minimal() {
        let manifest = PresetManifest {
            preset: PresetHeader {
                id: "roundtrip".into(),
                version: 1,
                kind: PresetKind::Creator,
                description: "test".into(),
                requires_capabilities: vec![],
                initial: "a".into(),
                terminal: "b".into(),
                author: None,
                homepage: None,
                license: None,
                initial_action: None,
            },
            states: vec![
                StateDefinition {
                    id: "a".into(),
                    description: None,
                    enter: vec![],
                    exit_when: Some(ExitWhen::Manual),
                    next: Some(NextTarget::Linear("b".into())),
                    terminal: false,
                    context_update: None,
                },
                StateDefinition {
                    id: "b".into(),
                    description: None,
                    enter: vec![],
                    exit_when: Some(ExitWhen::Rule),
                    next: None,
                    terminal: true,
                    context_update: None,
                },
            ],
            inner_graphs: None,
            signals: vec![],
            roles: vec![],
        };
        let yaml = serde_yaml::to_string(&manifest).unwrap();
        let back: PresetManifest = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.preset.id, "roundtrip");
        assert!(back.roles.is_empty());
    }
}
