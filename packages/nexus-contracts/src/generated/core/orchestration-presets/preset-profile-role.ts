/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * A role definition for multi-agent presets.
 */
export interface PresetProfileRole {
  id: string;
  description: string;
  systemPromptFile: string;
  recommendedSkills?: string[];
}
