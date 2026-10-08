/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { PresetProfileConditionalRule } from './preset-profile-conditional-rule';
import type { PresetProfileEnterAction } from './preset-profile-enter-action';
import type { PresetProfileExitWhen } from './preset-profile-exit-when';
import type { PresetProfileLabeledNext } from './preset-profile-labeled-next';
import type { PresetProfileLanes } from './preset-profile-lanes';
import type { PresetProfileNext } from './preset-profile-next';
import type { PresetProfileRole } from './preset-profile-role';
import type { PresetProfileSignal } from './preset-profile-signal';
import type { PresetProfileState } from './preset-profile-state';

/**
 * Response body for `GET /v1/daemon/orchestration/presets/{id}/profile` (AR-20..23): manifest-derived profile; manifest fields the preset does not carry serialize absent (AR-21).
 */
export interface PresetProfileResponse {
  id: string;
  version: number;
  sourceHash: string;
  lanes: PresetProfileLanes;
  states: PresetProfileState[];
  roles?: PresetProfileRole[];
  requiredCapabilities?: string[];
  signals?: PresetProfileSignal[];
}
