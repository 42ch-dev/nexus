/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Environment-local opaque subscription handle. It is bound to the admitted principal, the core generation that minted it and the one root run; a released or foreign handle refuses.
 */
export interface CoreWorkflowSubscription {
  /**
   * Opaque environment-local subscription UUID. Release it on disconnect so the run's subscriber permit is freed.
   */
  subscription_id: string;
}
