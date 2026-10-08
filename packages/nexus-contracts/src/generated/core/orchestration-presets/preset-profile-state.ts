/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { PresetProfileConditionalRule } from './preset-profile-conditional-rule';
import type { PresetProfileEnterAction } from './preset-profile-enter-action';
import type { PresetProfileExitWhen } from './preset-profile-exit-when';
import type { PresetProfileLabeledNext } from './preset-profile-labeled-next';
import type { PresetProfileNext } from './preset-profile-next';

/**
 * One state of the outer state machine; `exitWhen`/`next` are absent for terminal states.
 */
export interface PresetProfileState {
  id: string;
  description?: string;
  enter?: PresetProfileEnterAction[];
  exitWhen?: PresetProfileExitWhen;
  next?: PresetProfileNext;
  terminal: boolean;
}
