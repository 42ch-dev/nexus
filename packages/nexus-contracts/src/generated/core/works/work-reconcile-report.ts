/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Chapter reconcile counters (DB-as-SSOT resync); identical for the dry-run preview and the committed run.
 */
export interface WorkReconcileReport {
  created: number;
  updated: number;
  resynced: number;
  preserved: number;
}
