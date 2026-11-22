/**
 * @generated AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Terminal node ids.
 */
export type Terminal = string[];
/**
 * Document nodes.
 */
export type Nodes = WorkflowNode[];
/**
 * Document edges.
 */
export type Edges = WorkflowEdge[];
/**
 * Composition bindings; at most three unexpanded nest levels, no cycles.
 */
export type Nests = WorkflowNestBinding[];
/**
 * Preset-level World-binding requirement, carried from `preset.world_binding` (workflow-profile §5.3.4). Absent when the preset declares none.
 */
export type WorkflowWorldBinding = {
  /**
   * Whether a run must be bound to a World.
   */
  mode: "required" | "optional";
} | null;

/**
 * Body of POST /v1/daemon/strategies/{strategy_id}/generate, and the input contract of the `nexus.strategy.generate` agent tool. `request` is the GenerateRequest contract (D8): the pattern-package/IR subset a caller may declare, and nothing else — node declarations, edges, nest bindings and the meta fields semantic validation reads. It carries no executable code and no flag that bypasses lint, so a request can only ask for a document; whether that document is legal is decided by lint, never by the caller. The path `strategy_id` is authoritative and is not repeated in the body.
 */
export interface StrategyGenerateRequest {
  /**
   * Candidate revision the caller last read. The kernel commits `base_revision + 1`, or refuses with `strategy_candidate_conflict` and writes nothing — the same compare-and-swap Compose uses.
   */
  base_revision: number;
  request: GenerateRequest;
}
/**
 * The document to generate. Every node, edge and nest is the caller's; nothing is invented. The effect rollup is derived from the bytes under lint, never trusted from this object.
 */
export interface GenerateRequest {
  /**
   * Entry node id. Replaces `preset.initial`.
   */
  entry: string;
  terminal: Terminal;
  nodes: Nodes;
  edges: Edges;
  nests: Nests;
  meta: WorkflowMeta;
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
  world_binding?: WorkflowWorldBinding;
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
