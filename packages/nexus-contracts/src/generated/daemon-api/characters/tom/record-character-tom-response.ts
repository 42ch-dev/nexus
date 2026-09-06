/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/characters/{character_id}/tom. Returned only after the carrier CAS and the derivative MindState insert committed atomically; any failure rolls back both writes.
 */
export interface RecordCharacterTomResponse {
  /**
   * Carrier KnowledgeEntry id that was CAS-patched.
   */
  carrier_entry_id: string;
  /**
   * New carrier revision after the CAS bump (expected_revision + 1).
   */
  revision: number;
  /**
   * Derivative MindState row id; its holder_entry_id equals carrier_entry_id.
   */
  mind_state_id: string;
}
