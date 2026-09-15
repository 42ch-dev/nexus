/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Handler-local wire DTOs for the findings family without an existing schema (`findings-api` destination): the retention prune outcome. The PATCH tri-state (`rule_suggestion`) is owned by `schemas/daemon-api/findings/update-finding-request.schema.json` and must never be duplicated here (R-V1190-FINDINGS-TRISTATE-DUP: single wire definition, no parallel handwritten shape).
 */
export type FindingsApi = FindingsPruneResponse;

/**
 * Response for `POST /v1/daemon/findings/prune` (V1.49 P3, quality-loop §9.4). `count` is the number of rows deleted, or that would be deleted under `dry_run`.
 */
export interface FindingsPruneResponse {
  count: number;
  /**
   * Resolved retention threshold in days (bounded server default when the query omits it).
   */
  older_than_days: number;
  dry_run: boolean;
  /**
   * Server-side epoch second used for the cutoff.
   */
  now_epoch: number;
}
