/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/worlds. The daemon resolves the active creator; clients never send ownership.
 */
export interface CreateWorldRequest {
  /**
   * Human-readable World title. Trimmed and validated server-side (1-200 chars after trim).
   */
  title: string;
}
