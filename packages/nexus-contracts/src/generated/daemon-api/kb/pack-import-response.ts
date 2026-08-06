/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/worlds/:world_id/kb/pack/import (V1.152 P0 DF-77). Per-atom-type summary counts plus a details list.
 */
export interface PackImportResponse {
  entries: AtomCounts;
  relations: AtomCounts1;
  /**
   * Per-atom outcome details (entry or relation).
   */
  details: ImportDetail[];
}
/**
 * Entry-level counts.
 */
export interface AtomCounts {
  created: number;
  skipped: number;
  rejected: number;
  renamed: number;
  overwritten: number;
}
/**
 * Relation-level counts.
 */
export interface AtomCounts1 {
  created: number;
  skipped: number;
  rejected: number;
  renamed: number;
  overwritten: number;
}
/**
 * This interface was referenced by `PackImportResponse`'s JSON-Schema
 * via the `definition` "import_detail".
 */
export interface ImportDetail {
  kind: "entry" | "relation";
  id: string;
  outcome: "created" | "skipped" | "rejected" | "renamed" | "overwritten";
  /**
   * Optional human-readable reason (e.g. why rejected or skipped).
   */
  reason?: string;
}
/**
 * This interface was referenced by `PackImportResponse`'s JSON-Schema
 * via the `definition` "atom_counts".
 */
export interface AtomCounts2 {
  created: number;
  skipped: number;
  rejected: number;
  renamed: number;
  overwritten: number;
}
