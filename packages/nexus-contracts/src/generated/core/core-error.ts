/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Wire error envelope for nexus-core / native / service adapters (NexusApiError-compatible categories).
 */
export interface CoreError {
  code:
    | "uninitialized"
    | "auth_required"
    | "invalid_input"
    | "forbidden"
    | "not_found"
    | "world_kb_conflict"
    | "world_kb_validation"
    | "writer_fenced"
    | "owner_busy"
    | "schema_mismatch"
    | "busy"
    | "closing"
    | "interrupted"
    | "internal";
  message: string;
  details?: {
    [k: string]: unknown | undefined;
  };
  http_status?: number;
}
