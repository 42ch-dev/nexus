/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Success response for Strategy patch routes (V1.71). Returns the committed revision and any domain validation diagnostics produced during the patch.
 */
export interface StrategyPatchResponse {
  /**
   * Canonical graph revision after the patch was persisted.
   */
  new_revision: number;
  validation_summary: {
    /**
     * Fatal validation messages that prevented the patch. Always present; empty on success.
     */
    errors: string[];
    /**
     * Non-fatal validation messages. Always present; empty when none.
     */
    warnings: string[];
  };
  /**
   * Optional daemon-owned derived updates (e.g. normalized references).
   */
  side_effects?: string[];
}
