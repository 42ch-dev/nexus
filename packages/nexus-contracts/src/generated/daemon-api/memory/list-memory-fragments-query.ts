/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Query parameters for GET /v1/daemon/memory/fragments. `keyword` is an optional case-insensitive LIKE filter; `limit` defaults to 50 (clamped 1..=250) when omitted.
 */
export interface ListMemoryFragmentsQuery {
  creator_id: string;
  /**
   * Optional world projection filter. Omitted returns the whole Creator SOUL; present returns only fragments that emerged from this world. V1.81 has no public query value for core-only fragments.
   */
  world_id?: string;
  keyword?: string;
  limit?: number;
}
