/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/presets/{id} (V1.65 P0). Replaces the user preset's preset.yaml content after validation.
 */
export interface UpdatePresetRequest {
  /**
   * Complete replacement preset.yaml content.
   */
  yaml: string;
}
