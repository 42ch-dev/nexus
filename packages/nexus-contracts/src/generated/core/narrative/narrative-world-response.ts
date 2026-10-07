/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { NarrativeWorldState } from './narrative-world-state';

/**
 * Response for `GET /v1/daemon/narrative/worlds/{world_id}`; 404 for an unknown World id.
 */
export interface NarrativeWorldResponse {
  world: NarrativeWorldState;
}
