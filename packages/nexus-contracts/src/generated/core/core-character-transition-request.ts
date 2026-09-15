/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Direct Character lifecycle transition request. Old activity tokens are invalid after transition; a no-op transition retires no sessions and advances no epoch.
 */
export interface CoreCharacterTransitionRequest {
  /**
   * Character ID: lowercase chr_ prefix and exactly 32 hex characters.
   */
  character_id: string;
  /**
   * Lifecycle target; archived or restored active.
   */
  target_status: "archived" | "active";
  /**
   * Optimistic concurrency revision of the Character record.
   */
  expected_revision: number;
}
