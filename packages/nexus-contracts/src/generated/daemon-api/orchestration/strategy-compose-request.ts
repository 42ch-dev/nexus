/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Body of POST /v1/daemon/strategies/{strategy_id}/compose. Exactly one composition operation, guarded by the candidate revision the client last read. The path `strategy_id` is authoritative: raw document bytes and multi-operation blobs are not accepted.
 */
export type StrategyComposeRequest = NestCompose | UnnestCompose | ReplaceSubtreeCompose | BindSlotCompose;

/**
 * Pin one library package as a new collapsed slot under `parent_node`.
 */
export interface NestCompose {
  /**
   * Candidate revision the client last read. The write commits `base_revision + 1`, or is refused as `strategy_candidate_conflict` with no write.
   */
  base_revision: number;
  op: "nest";
  payload: NestPayload;
}
export interface NestPayload {
  /**
   * Stable nest id; the region address for `unnest`, `replace_subtree` and `bind_slot`.
   */
  nest_id: string;
  /**
   * Id of the collapsed `pattern_slot` node this operation inserts.
   */
  parent_node: string;
  package: NestPackage;
  /**
   * When the region expands into concrete nodes and edges.
   */
  expansion: "defer_until_apply" | "expand_in_candidate";
}
export interface NestPackage {
  /**
   * Pattern registry id, which is also the package directory name.
   */
  id: string;
  /**
   * Exact version label from the package manifest. The daemon resolves the content hash; the caller never supplies one.
   */
  version: string;
}
/**
 * Flatten the region at `nest_id` out of the candidate.
 */
export interface UnnestCompose {
  /**
   * Candidate revision the client last read. The write commits `base_revision + 1`, or is refused as `strategy_candidate_conflict` with no write.
   */
  base_revision: number;
  op: "unnest";
  payload: UnnestPayload;
}
export interface UnnestPayload {
  /**
   * Nest id of the region to flatten.
   */
  nest_id: string;
}
/**
 * Re-pin the region at `nest_id` to another exact package, in place.
 */
export interface ReplaceSubtreeCompose {
  /**
   * Candidate revision the client last read. The write commits `base_revision + 1`, or is refused as `strategy_candidate_conflict` with no write.
   */
  base_revision: number;
  op: "replace_subtree";
  payload: ReplaceSubtreePayload;
}
export interface ReplaceSubtreePayload {
  /**
   * Nest id of the region to re-pin; the region keeps its slot node, bindings and graph position.
   */
  nest_id: string;
  package: ReplaceSubtreePackage;
  /**
   * When the re-pinned region expands into concrete nodes and edges.
   */
  expansion: "defer_until_apply" | "expand_in_candidate";
}
export interface ReplaceSubtreePackage {
  /**
   * Pattern registry id, which is also the package directory name.
   */
  id: string;
  /**
   * Exact version label from the package manifest. The daemon resolves the content hash; the caller never supplies one.
   */
  version: string;
}
/**
 * Bind one slot of the pattern pinned at `nest_id`.
 */
export interface BindSlotCompose {
  /**
   * Candidate revision the client last read. The write commits `base_revision + 1`, or is refused as `strategy_candidate_conflict` with no write.
   */
  base_revision: number;
  op: "bind_slot";
  payload: BindSlotPayload;
}
export interface BindSlotPayload {
  /**
   * Nest id whose pinned pattern declares the slot.
   */
  nest_id: string;
  /**
   * Slot name from the pinned package manifest.
   */
  slot: string;
  value: unknown;
}
