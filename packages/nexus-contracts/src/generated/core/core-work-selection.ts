/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Owned record of the Work selected for the caller's creator/workspace scope after select_work; selection is persisted, not session-local.
 */
export interface CoreWorkSelection {
  /**
   * Id of the selected Work.
   */
  work_id: string;
  /**
   * True when the Work is the active selection for the scope.
   */
  active: boolean;
}
