/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Owned query for the cross-World timeline overview. Keyset cursor pages by world id; the opaque cursor is bounded.
 */
export interface CoreTimelineOverviewQuery {
  /**
   * Opaque keyset cursor from a previous page; omitted on the first page.
   */
  cursor?: string;
}
