/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/findings/stale.
 */
export interface StaleFindingsResponse {
  open_count: number;
  stale_threshold_seconds: number;
  items: {
    [k: string]: unknown | undefined;
  }[];
}
