/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for `POST /v1/daemon/findings/prune` (V1.49 P3): `count` is rows deleted, or that would be deleted under `dry_run`. The PATCH tri-state stays in update-finding-request.schema.json (R-V1190-FINDINGS-TRISTATE-DUP: single wire definition).
 */
export interface FindingsPruneResponse {
  count: number;
  older_than_days: number;
  dry_run: boolean;
  now_epoch: number;
}
