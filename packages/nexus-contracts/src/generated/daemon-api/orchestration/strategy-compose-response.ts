/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Body of a committed compose operation (HTTP 200). `document` is the complete committed candidate at `new_revision`, resolved pins included, so the client replaces its candidate with this document instead of predicting server normalization.
 */
export interface StrategyComposeResponse {
  /**
   * Revision the operation committed; always the request's `base_revision` plus one.
   */
  new_revision: number;
  document: NexusWorkflowDocument;
  /**
   * Committed validator diagnostics, even when empty. A diagnostic is reported, never defaulted away.
   */
  validation_summary: StrategyComposeDiagnostic[];
}
/**
 * Workflow IR v0 — the single source of truth for the composed workflow document. A distinct document from `preset.yaml`: YAML is a projection, never a second grammar. Executable preset carriers (signals, roles, initial_action, cli_args, output_binding, every exit_when kind) are preserved; canvas-only nests, lifecycle state, effect fence and provenance are IR-only. `ir_version` stays 0 until P1 parity.
 */
export interface NexusWorkflowDocument {
  /**
   * Workflow IR document version. 0 is the v0 draft (P0); a bump is required for any nest-rule or hash-membership change.
   */
  ir_version: number;
  /**
   * Strategy identifier; same id rules as `preset.id` today.
   */
  id: string;
  /**
   * Authored document version, preserved for projection back to `preset.version`. Not a content identity.
   */
  version: number;
  /**
   * Mutable candidate CAS version used by Canvas/API patches. A revision alone never proves content identity.
   */
  revision: number;
  /**
   * BLAKE3 tree hash over canonical IR plus the resolved pattern/asset hash set. Null until computed; approval and Apply bind this value.
   */
  content_hash?: string | null;
  /**
   * Document lifecycle state. Candidates are mutable; approved and applied snapshots are frozen (a new candidate is required to change one). Silent candidate→applied promotion is not representable.
   */
  state: "candidate" | "approved" | "applied_ref";
  meta: WorkflowMeta;
  /**
   * Entry node id. Replaces `preset.initial`.
   */
  entry: string;
  initial_action?: WorkflowInitialAction;
  /**
   * Terminal node ids.
   */
  terminal: string[];
  /**
   * Document nodes.
   */
  nodes: WorkflowNode[];
  /**
   * Document edges.
   */
  edges: WorkflowEdge[];
  /**
   * Composition bindings; at most three unexpanded nest levels, no cycles.
   */
  nests: WorkflowNestBinding[];
  /**
   * Referenced assets.
   */
  assets: WorkflowAssetRef[];
  /**
   * Signal bindings.
   */
  signals: WorkflowSignalBinding[];
  /**
   * Role definitions.
   */
  roles: WorkflowRoleDefinition[];
  /**
   * Rolled-up effect fence for Simulate/Approve: the lattice join of every node and nested child effect class. Nesting never hides a write or external effect.
   */
  effect_summary: "pure" | "read" | "write_candidate" | "external";
  provenance: WorkflowProvenance;
}
/**
 * Document metadata carried from the manifest header and enqueue-time gate declarations.
 */
export interface WorkflowMeta {
  /**
   * Human-readable description.
   */
  description: string;
  /**
   * Creator-facing or internal system strategy.
   */
  kind: "creator" | "system";
  /**
   * Optional author name.
   */
  author?: string | null;
  /**
   * Optional license identifier.
   */
  license?: string | null;
  /**
   * Optional homepage/docs URL (non-executable).
   */
  homepage?: string | null;
  /**
   * Declared run intents.
   */
  run_intents: ("work_init" | "work_continue" | "knowledge_ingest" | "work_maintenance" | "system_maintenance")[];
  /**
   * Capabilities the workflow requires; resolution fails when any is missing.
   */
  requires_capabilities: string[];
  /**
   * Preset-specific CLI argument declarations.
   */
  cli_args?: WorkflowCliArg[];
  /**
   * Enqueue-time gate declarations.
   */
  gates?: WorkflowGate[];
}
/**
 * Preset-specific CLI argument declaration, preserved for the generic runner.
 */
export interface WorkflowCliArg {
  /**
   * Flag name without the `--` prefix.
   */
  name: string;
  /**
   * Value type used for parsing and JSON coercion.
   */
  type: "string" | "integer" | "boolean";
  /**
   * Whether the flag is required.
   */
  required?: boolean | null;
  default?: unknown;
  /**
   * Help text shown in `--help` output.
   */
  description?: string | null;
}
/**
 * Enqueue-time precondition. Gates remain enqueue-time; ownership stays with the schedule Master.
 */
export interface WorkflowGate {
  /**
   * Gate family.
   */
  kind: "work_field" | "filesystem" | "previous_preset";
  /**
   * Work-table column for `work_field` gates.
   */
  field?: string | null;
  /**
   * Comparison operator for `work_field` gates.
   */
  op?: "equals" | "not_equals" | "required" | "in" | "not_in";
  value?: unknown;
  /**
   * Path template for `filesystem` gates.
   */
  path?: string | null;
  /**
   * Whether the path must exist (`filesystem` gates).
   */
  must_exist?: boolean | null;
  /**
   * Prior preset id (`previous_preset` gates).
   */
  preset?: string | null;
  /**
   * Required status of the prior preset (`previous_preset` gates).
   */
  status?: "complete" | "any_session";
  /**
   * Gate scope; only `work` is normative.
   */
  scope?: string | null;
}
/**
 * Seeds `core_context` when a run starts (today `preset.initial_action`).
 */
export interface WorkflowInitialAction {
  /**
   * Use the seed directly, or expand it through a capability first.
   */
  kind: "seed_direct" | "seed_expansion";
  /**
   * Expansion capability (seed_expansion only).
   */
  capability?: string | null;
  /**
   * Asset id of the expansion prompt template (seed_expansion only).
   */
  template?: string | null;
  /**
   * Expected payload kind after expansion.
   */
  payload_kind?: string | null;
}
/**
 * A document node. `kind` selects the role; kind-specific fields are optional so one record shape carries outer states, inner steps, pattern slots, nest groups and control nodes.
 */
export interface WorkflowNode {
  /**
   * Unique node id within this document.
   */
  id: string;
  /**
   * Node role in the composed graph.
   */
  kind: "outer_state" | "inner_step" | "pattern_slot" | "nest_group" | "join" | "terminal" | "manual_wait" | "judge";
  /**
   * Worst-case effect this node can cause.
   */
  effect_class: "pure" | "read" | "write_candidate" | "external";
  ui?: WorkflowNodeUi;
  /**
   * Actions executed when the node is entered.
   */
  enter?: WorkflowEnterAction[];
  exit_when?: WorkflowExitWhen;
  converge?: WorkflowConverge;
  merge?: WorkflowMerge;
  context_update?: WorkflowContextUpdate;
  /**
   * Bounded-join deadline in milliseconds (join nodes only).
   */
  timeout_ms?: number | null;
  /**
   * Node id to reroute to when the deadline fires.
   */
  on_timeout?: string | null;
  /**
   * Inner-step executor kind.
   */
  step_kind?: "acp_prompt";
  /**
   * Asset id of the prompt/template this step renders.
   */
  template?: string | null;
  /**
   * Tool policy for the step's ACP prompt.
   */
  tool_policy?: string | null;
  /**
   * Node ids this step depends on (inner-graph DAG).
   */
  depends_on?: string[];
  /**
   * Role id bound to this step; must match a `roles[].id`.
   */
  role?: string | null;
  /**
   * Which subgraph output is exported as the named state output (e.g. `select.text`).
   */
  output_binding?: string | null;
  pattern?: WorkflowPatternRef;
  /**
   * Pattern slot bindings (slot name → bound node/asset/parameter).
   */
  bind?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Child workflow id for a nest group.
   */
  child_workflow_id?: string | null;
  /**
   * Canvas collapse flag for a nest group; never affects execution.
   */
  collapsed_view?: boolean | null;
}
/**
 * Canvas chrome only; must not affect execution semantics.
 */
export interface WorkflowNodeUi {
  /**
   * Canvas label.
   */
  label?: string | null;
  /**
   * Canvas description.
   */
  description?: string | null;
}
/**
 * Action executed on entering the node.
 */
export interface WorkflowEnterAction {
  /**
   * Action family.
   */
  kind: "capability" | "inner_graph" | "host_tool";
  /**
   * Capability name, or inner graph name for `inner_graph`.
   */
  name?: string | null;
  /**
   * Daemon host tool name (`host_tool` only).
   */
  tool_name?: string | null;
  args?: unknown;
}
/**
 * Condition that must hold before the node advances. Covers every preset exit_when kind.
 */
export interface WorkflowExitWhen {
  /**
   * Exit condition family.
   */
  kind: "manual" | "rule" | "llm_judge" | "graph_complete" | "timer";
  /**
   * Asset id of the judge prompt template (llm_judge only).
   */
  template?: string | null;
  /**
   * Judge capability to call (llm_judge only).
   */
  judge_capability?: string | null;
  /**
   * Minimum ISO-8601 interval between re-evaluations (llm_judge only).
   */
  min_interval?: string | null;
  /**
   * ISO-8601 wait duration (timer only).
   */
  duration?: string | null;
}
/**
 * Dedicated merge-point convergence strategy.
 */
export interface WorkflowConverge {
  /**
   * Merge-point strategy.
   */
  strategy: "wait_for_all" | "first_completed" | "any";
}
/**
 * Merge semantics for a node with multiple incoming labeled edges.
 */
export interface WorkflowMerge {
  /**
   * Merge kind.
   */
  kind: "all" | "any" | "quorum";
  /**
   * Minimum arrivals (quorum only).
   */
  n?: number | null;
  /**
   * Expected incoming edges (quorum only).
   */
  m?: number | null;
}
/**
 * Hook fired on node exit that edits the schedule's core_context. Preset-sourced hooks are additive only.
 */
export interface WorkflowContextUpdate {
  /**
   * Context edit operation.
   */
  op: "append" | "struct_merge" | "replace" | "struct_remove" | "llm_summarize";
  /**
   * Asset id of the edit-content template.
   */
  template?: string | null;
  /**
   * Body content (append/replace).
   */
  body?: string | null;
  /**
   * Removal path (struct_remove).
   */
  path?: string | null;
  patch?: unknown;
  /**
   * Summarize capability (llm_summarize).
   */
  capability?: string | null;
}
/**
 * Exact pattern pin carried by an unexpanded pattern slot.
 */
export interface WorkflowPatternRef {
  /**
   * Pattern registry id.
   */
  registry_id: string;
  /**
   * Exact pattern version.
   */
  version: string;
  /**
   * Pattern content hash recorded when resolved.
   */
  content_hash?: string | null;
}
/**
 * A directed transition between nodes.
 */
export interface WorkflowEdge {
  /**
   * Unique edge id.
   */
  id: string;
  /**
   * Source node id.
   */
  from: string;
  /**
   * Target node id.
   */
  to: string;
  /**
   * Transition family: linear, judge GO/NOGO, expression-conditional, fallback, or inner-graph dependency.
   */
  kind: "linear" | "go" | "nogo" | "conditional" | "default" | "depends_on";
  /**
   * Judge label selecting this edge (N-way labeled routing).
   */
  label?: string | null;
  /**
   * Rule expression guarding this edge.
   */
  when?: string | null;
}
/**
 * First-class composition binding. `id` is the stable address for atomic replace-subtree-by-nest_id.
 */
export interface WorkflowNestBinding {
  /**
   * Stable nest id; the region address for partial regeneration.
   */
  id: string;
  /**
   * Node id of the pattern slot or nest group that owns this nest.
   */
  parent_node: string;
  /**
   * When the nest expands into concrete nodes and edges.
   */
  expansion: "defer_until_apply" | "expand_in_candidate";
  /**
   * Pin policy. v0 supports exact pins only.
   */
  pin_policy: "exact";
  child: WorkflowNestChild;
}
/**
 * Nested child: an exact pattern pin or a child workflow reference.
 */
export interface WorkflowNestChild {
  /**
   * Nested child family.
   */
  kind: "pattern" | "workflow";
  /**
   * Pattern registry id (pattern children).
   */
  registry_id?: string | null;
  /**
   * Exact pattern version (pattern children).
   */
  version?: string | null;
  /**
   * Child content hash recorded when resolved.
   */
  content_hash?: string | null;
  /**
   * Child workflow id (workflow children).
   */
  workflow_id?: string | null;
  /**
   * Child workflow revision (workflow children).
   */
  workflow_revision?: number | null;
}
/**
 * Referenced template/prompt asset. `id` is what nodes and hooks reference; `path` stays bundle-relative for projection.
 */
export interface WorkflowAssetRef {
  /**
   * Asset id referenced by nodes, exit_when and context_update hooks.
   */
  id: string;
  /**
   * Bundle-relative asset path.
   */
  path: string;
  /**
   * Asset family.
   */
  kind: "template" | "prompt" | "other";
  /**
   * Content hash participating in the approved hash set.
   */
  content_hash?: string | null;
}
/**
 * Executable external signal binding.
 */
export interface WorkflowSignalBinding {
  /**
   * Signal name.
   */
  name: string;
  /**
   * What the signal does on receipt.
   */
  action: "pause" | "force_transition";
  /**
   * Target node id (force_transition only).
   */
  target?: string | null;
}
/**
 * Executable role definition for multi-agent workflows.
 */
export interface WorkflowRoleDefinition {
  /**
   * Unique role id within this document.
   */
  id: string;
  /**
   * Human-readable role description.
   */
  description: string;
  /**
   * Bundle-relative system prompt path.
   */
  system_prompt_file: string;
  /**
   * Ordered skill slugs; first entry is primary.
   */
  recommended_skills?: string[];
}
/**
 * How this document came to be, including the resolved pin set an approval binds.
 */
export interface WorkflowProvenance {
  /**
   * Document origin. `generate` is data only in P0; no generator runtime exists.
   */
  source: "generate" | "compose" | "import_preset";
  /**
   * Agent run that produced or patched the document.
   */
  agent_run_id?: string | null;
  /**
   * Lineage link for partial regeneration.
   */
  parent_candidate_id?: string | null;
  /**
   * Resolved pattern pins; Apply verifies them.
   */
  resolved_pins?: WorkflowResolvedPin[];
}
/**
 * A pattern pin resolved for this document.
 */
export interface WorkflowResolvedPin {
  /**
   * Pattern registry id.
   */
  registry_id: string;
  /**
   * Exact pattern version.
   */
  version: string;
  /**
   * Resolved pattern content hash.
   */
  content_hash?: string | null;
}
export interface StrategyComposeDiagnostic {
  /**
   * Diagnostic family.
   */
  code: "unbound_required_slot";
  /**
   * The nest that pins the package.
   */
  nest_id: string;
  /**
   * The pinned package (present for `unbound_required_slot`).
   */
  package_id?: string;
  /**
   * The pinned package version (present for `unbound_required_slot`).
   */
  package_version?: string;
  /**
   * The unbound slot name (present for `unbound_required_slot`).
   */
  slot?: string;
  /**
   * Human-readable diagnostic safe for UI display.
   */
  message: string;
}
