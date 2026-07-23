/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/sync/pull — incremental bundle fetch from the platform (CLI/daemon client contract).
 */
export interface SyncPullRequest {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Return bundles with server delta sequence strictly greater than this cursor (incremental pull).
   */
  after_confirmed_delta_sequence?: number;
}
