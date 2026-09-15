/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Handler-local wire DTOs for the Work authoring family (`works-api` destination): authoring pool entries/list, pool set-active/promote/archive, inspiration items/list/add/promote/archive and the chapter reconcile report. `creator_id` never crosses the wire: the surface is local-first and always the active creator (R-V141P1-11). Wire shapes mirror the retired daemon-local envelopes verbatim.
 */
export type WorksApi = WorkPoolListResponse;

/**
 * Offset-paginated authoring-pool page (legacy `list_pool` envelope).
 */
export interface WorkPoolListResponse {
  entries: WorkPoolEntry[];
  total: number;
  limit: number;
  offset: number;
}
/**
 * One authoring-pool entry as served on the wire (the stored `creator_id` is intentionally not serialized).
 */
export interface WorkPoolEntry {
  /**
   * Authoring-pool entry id.
   */
  entry_id: string;
  /**
   * Work the entry points at; empty when the entry has no Work yet.
   */
  work_id: string;
  /**
   * Entry status (`active` / `queued` / `archived`).
   */
  status: string;
  title: string;
  /**
   * RFC 3339 promotion timestamp.
   */
  promoted_at: string;
  /**
   * Optional entry note.
   */
  note?: string;
}
