/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Canonical wire DTOs for the `/v1/daemon/orchestration/presets*` read surface (`orchestration-presets-api` destination). Migrated verbatim from the retired handwritten `nexus-contracts::local::orchestration::http` preset definitions: wire casing stays camelCase, optional manifest fields stay absent-when-not-carried (AR-21), and the trigger-lane booleans stay the flat PL-3 vocabulary the CLI `preset show --json` output matches verbatim (AR-25). Deliberately NOT part of this contract: an update-request `expected_source_hash` — the existing `UpdatePresetRequest` stays YAML-only and the strategy revision CAS remains the accepted write guard (P3-T4 request declined without evidence of an existing versioned update contract).
 */
export type OrchestrationPresetsApi = OrchestrationPresetListResponse;

/**
 * Response body for `GET /v1/daemon/orchestration/presets` (loadable preset ids). Named `OrchestrationPresetListResponse` because `preset-management` already owns the canonical `ListPresetsResponse` wire shape (`embedded`/`system`/`user`), and the two shapes are not interchangeable.
 */
export interface OrchestrationPresetListResponse {
  presets: string[];
}
