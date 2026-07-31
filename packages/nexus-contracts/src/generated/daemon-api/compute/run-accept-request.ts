/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/compute/runs/{run_id}/accept — atomically commit a succeeded Run's proposals into the World (V1.147 P0 direct lane).
 */
export interface RunAcceptRequest {
  /**
   * Optional subset of timeline event IDs from the proposals to accept. All timeline events are accepted when this field is absent or null. State updates and new knowledge entries are always all-or-nothing with the Accept action.
   */
  timeline_event_ids_to_accept?: string[];
}
