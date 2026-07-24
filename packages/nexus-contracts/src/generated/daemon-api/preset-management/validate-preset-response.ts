/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/presets:validate — validation result with structured errors and warnings.
 */
export interface ValidatePresetResponse {
  valid: boolean;
  id?: string;
  version?: number;
  state_count?: number;
  errors: string[];
  warnings?: string[];
}
