/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Offset-paginated authoring-pool page for `GET /v1/daemon/works/pool` (legacy `list_pool` envelope).
 */
export interface WorkPoolListResponse {
  entries: WorkPoolEntry[];
  total: number;
  limit: number;
  offset: number;
}
/**
 * One authoring-pool entry as served on the wire (stored `creator_id` intentionally not serialized: local-first surface, always the active creator, R-V141P1-11).
 */
export interface WorkPoolEntry {
  entry_id: string;
  work_id: string;
  status: string;
  title: string;
  promoted_at: string;
  note?: string;
}
