/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Structured detail placed inside the canonical ErrorResponse.details field when a Strategy patch is rejected because base_revision is stale (HTTP 409).
 */
export interface StrategyConflictError {
  /**
   * Current canonical revision of the Strategy graph.
   */
  current_revision: number;
  /**
   * Identifier of the node or subresource involved in the conflict.
   */
  node_id: string;
  /**
   * Dot-path or locator describing the field that changed underneath the client.
   */
  conflicting_path: string;
  /**
   * Actionable hint for the client (e.g. 'refetch and reapply').
   */
  recovery_hint: string;
}
