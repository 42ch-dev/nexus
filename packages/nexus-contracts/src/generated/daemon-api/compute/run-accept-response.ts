/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/compute/runs/{run_id}/accept — result of atomically applying a Run's proposals (V1.147 P0 direct lane).
 */
export interface RunAcceptResponse {
  /**
   * Run identifier that was accepted.
   */
  run_id: string;
  /**
   * Always "applied" on success.
   */
  status: "applied";
  /**
   * Summary counts of what was applied in the atomic transaction.
   */
  applied: {
    /**
     * Number of state delta operations applied to existing KnowledgeEntries.
     */
    state_delta_count: number;
    /**
     * Number of Timeline events created (event_type: "compute_result").
     */
    events_created: number;
    /**
     * Number of new KnowledgeEntry records created from new_key_blocks.
     */
    new_entries_created: number;
  };
  /**
   * IDs of the Timeline events created during accept, in the order they were appended.
   */
  timeline_event_ids: string[];
}
