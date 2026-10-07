/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { PresetProfileConditionalRule } from './preset-profile-conditional-rule';
import type { PresetProfileLabeledNext } from './preset-profile-labeled-next';

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
