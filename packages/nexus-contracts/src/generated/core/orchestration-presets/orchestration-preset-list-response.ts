/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for `GET /v1/daemon/orchestration/presets` (loadable preset ids). Named `OrchestrationPresetListResponse` because `preset-management` owns the canonical `ListPresetsResponse` wire shape and the two are not interchangeable.
 */
export interface OrchestrationPresetListResponse {
  presets: string[];
}
