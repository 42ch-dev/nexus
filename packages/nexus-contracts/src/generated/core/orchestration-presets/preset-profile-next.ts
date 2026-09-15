/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Next transition form (`linear` / `goNogo` / `labeled` / `conditional` / `branches`).
 */
export interface PresetProfileNext {
  kind: string;
  target?: string;
  go?: string;
  nogo?: string;
  labeled?: PresetProfileLabeledNext[];
  rules?: PresetProfileConditionalRule[];
  branches?: PresetProfileConditionalRule[];
  default?: string;
}
/**
 * A labeled next edge (`labeled` form).
 */
export interface PresetProfileLabeledNext {
  label: string;
  target: string;
}
/**
 * A conditional rule (expression -> target edge).
 */
export interface PresetProfileConditionalRule {
  when: string;
  target: string;
}
