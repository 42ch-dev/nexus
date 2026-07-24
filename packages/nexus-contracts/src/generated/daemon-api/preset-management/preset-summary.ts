/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Summary of a single preset entry (id, source, run intents).
 */
export interface PresetSummary {
  id: string;
  source: "embedded" | "system" | "user";
  run_intents?: string[];
}
