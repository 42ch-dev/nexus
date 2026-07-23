/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/presets/{id} (V1.65 P0). Returns the preset manifest as raw YAML so clients can edit and PATCH it back.
 */
export interface GetPresetResponse {
  id: string;
  source: "embedded" | "system" | "user";
  /**
   * Absolute filesystem path for system/user presets; empty for embedded presets.
   */
  path?: string;
  /**
   * Raw preset.yaml content.
   */
  yaml: string;
}
