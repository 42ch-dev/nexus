/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Deterministic trajectory of a candidate workflow document under a static walk (Simulate output). Produced from the request's candidate IR; nothing executes, no provider is called, and the input document is not mutated. The same IR yields the same bytes twice.
 */
export interface SimulationTrajectory {
  /**
   * Trajectory contract revision, fixed at 1 for this shape. A structural change bumps it; adding a `simulation_level` value does not.
   */
  schema_version: 1;
  /**
   * BLAKE3 canonical content hash recomputed over the request's candidate IR. Never echoed from an optional request `content_hash`.
   */
  ir_content_hash: string;
  /**
   * Open string naming how deeply the walk explored the graph. `reachability` is the only value v0 emits: judge branches are not explored (each `llm_judge` follows its declared default edge and is listed in `stubbed_judges`) and no provider is called. Consumers MUST treat an unknown value as a newer, unsupported level instead of failing deserialization or assuming `reachability`.
   */
  simulation_level: string;
  /**
   * Node ids reached from `entry`, sorted lexicographically in `str` order (not traversal order), so the same IR yields the same list across runs.
   */
  nodes_visited: string[];
  /**
   * Static parking points reachable from `entry`.
   */
  predicted_waits: SimulationPredictedWait[];
  /**
   * Worst-case effect fence over the visited default-branch nodes and their nested children, compared by the total order `pure < read < write_candidate < external` (a parent is the maximum of its children). The generated Rust enum derives `Ord`/`PartialOrd` in exactly this declaration order, so consumers compare with those derives instead of a local rank table.
   */
  effect_summary: "pure" | "read" | "write_candidate" | "external";
  /**
   * Node ids visited through an `exit_when.kind: llm_judge` whose declared default branch was followed without a model call.
   */
  stubbed_judges: string[];
}
/**
 * A statically predicted parking point. Static over-approximation: the wait may never fire at run time.
 */
export interface SimulationPredictedWait {
  /**
   * Why the walk stops: a manual wait node, or a `pause` signal binding.
   */
  reason: "manual" | "signal";
  /**
   * Node id when `reason` is `manual`; signal name for a global `signals[].action: pause` binding, which has no target node.
   */
  subject: string;
}
