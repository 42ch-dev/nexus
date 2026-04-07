import type { ManuscriptPhase, SchemaVersion } from './CommonTypes';

/**
 * Nexus ManuscriptState
 *
 * ManuscriptState - local-only manuscript phase machine tracking creation progression. Platform may receive manuscript_phase as bundle metadata but does not own this aggregate in V1.0. Aligned with data-model-v1.md §5.9B.
 *
 * @schema_version 1
 * @source manuscript-state.schema.json
 */
/** ManuscriptState - local-only manuscript phase machine tracking creation progression. Platform may receive manuscript_phase as bundle metadata but does not own this aggregate in V1.0. Aligned with data-model-v1.md §5.9B. */
export interface ManuscriptState {
  schema_version: number;
  manuscript_state_id: string;
  workspace_id: string;
  world_id: string;
  creator_id: string;
  manuscript_phase: ManuscriptPhase;
  active_manifest_id?: string;
  last_confirmed_delta_sequence?: number;
  updated_at: string;
}
