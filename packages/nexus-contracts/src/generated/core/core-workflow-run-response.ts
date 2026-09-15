/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Authoritative workflow run record projection: run identity, status and the CAS state revision anchoring workflow control. Also returned by cancel.
 */
export interface CoreWorkflowRunResponse {
  /**
   * Run (session) identity.
   */
  run_id: string;
  /**
   * Authoritative orchestration run status.
   */
  status: string;
  /**
   * Current state revision (CAS anchor).
   */
  state_revision: number;
}
