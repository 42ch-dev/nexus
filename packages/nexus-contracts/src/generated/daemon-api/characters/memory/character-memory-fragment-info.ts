/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One Character memory fragment. `binding_id` is the binding-local provenance; absent means shared Character scope. `revision` backs optimistic-concurrency promotion (local to shared).
 */
export interface CharacterMemoryFragmentInfo {
  fragment_id: string;
  session_id: string;
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
  /**
   * Binding-local provenance; omitted for shared Character scope.
   */
  binding_id?: string;
  summary: string;
  keywords: string[];
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
  /**
   * Optional TTL hint (e.g. `30d`).
   */
  ttl?: string;
  /**
   * OCC revision; required as `expected_revision` for promotion.
   */
  revision: number;
}
