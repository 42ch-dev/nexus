/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Exit condition kind (`llm_judge` / `rule` / `graph_complete` / `manual` / `timer`).
 */
export interface PresetProfileExitWhen {
  kind: string;
  templateFile?: string;
  judgeCapability?: string;
  minInterval?: string;
  duration?: string;
}
