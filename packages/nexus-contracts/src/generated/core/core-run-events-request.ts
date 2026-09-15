/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Bounded run-events pull request. Run events keep their own cursor/terminal source, distinct from provider events and core changes.
 */
export interface CoreRunEventsRequest {
  run_id: string;
  /**
   * Non-negative decimal string sequence cursor (exclusive lower bound).
   */
  after_sequence?: string;
  /**
   * Maximum events in this page.
   */
  limit?: number;
}
