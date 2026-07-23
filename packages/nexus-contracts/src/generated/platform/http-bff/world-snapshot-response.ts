/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/worlds/snapshot — snapshot anchor and revision metadata.
 */
export interface WorldSnapshotResponse {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Server world revision at capture time
   */
  world_revision: number;
  /**
   * Resolved anchor event for this snapshot
   */
  at_event_id?: string;
  /**
   * When the snapshot was taken (server clock, RFC 3339)
   */
  captured_at: string;
}
