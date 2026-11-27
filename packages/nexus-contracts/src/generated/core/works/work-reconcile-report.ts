/**
 * @generated AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Chapter reconcile counters (DB-as-SSOT resync). The counters describe one reconcile diff, which is exactly what the dry-run preview reports. The committed run can differ only by an apply-time skip: a chapter that stopped rendering between the diff and the write is subtracted from `resynced` and listed in `skipped`.
 */
export interface WorkReconcileReport {
  created: number;
  updated: number;
  resynced: number;
  preserved: number;
  skipped?: {
    path: string;
    reason: string;
  }[];
}
