/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/presets — presets grouped by source (embedded, system, user).
 */
export interface ListPresetsResponse {
  embedded: NexusPresetSummary[];
  system: NexusPresetSummary[];
  user: NexusPresetSummary[];
}
/**
 * Summary of a single preset entry (id, source, run intents).
 */
export interface NexusPresetSummary {
  id: string;
  source: "embedded" | "system" | "user";
  run_intents?: string[];
}
