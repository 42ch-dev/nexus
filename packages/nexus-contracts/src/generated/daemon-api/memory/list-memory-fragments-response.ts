/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for GET /v1/daemon/memory/fragments. Fragments are produced only by the `review` route (no CRUD on this surface). Unlike the pending-review list, this response is NOT paginated (returns up to `limit` rows).
 */
export interface ListMemoryFragmentsResponse {
  fragments: NexusMemoryFragmentInfo[];
}
/**
 * A single memory-fragment row in the list-fragments response. V1.79 exposes keyword and creation-time metadata for read-only SOUL visualization; write-only/internal fragment fields (session_id, creator_id, ttl) remain out of this response.
 */
export interface NexusMemoryFragmentInfo {
  fragment_id: string;
  summary: string;
  /**
   * Optional originating world identifier. Absent or null means a Creator-core-only fragment with no originating world; a string value means the fragment emerged from that world.
   */
  world_id?: string | null;
  /**
   * Optional keyword labels extracted for this fragment, decoded from the `memory_fragments.keywords` JSON array for read-only visualization.
   */
  keywords?: string[];
  /**
   * Optional RFC 3339 creation timestamp copied from `memory_fragments.created_at`; used as the temporal-drift axis. Family convention keeps timestamps as plain strings.
   */
  created_at?: string;
}
