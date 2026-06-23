import type { SchemaVersion, TimePolicy, Visibility, WorldStatus } from '../common/CommonTypes';
/**
 * Nexus World Entity
 *
 * World entity - a narrative universe maintained by creators with timeline evolution. Aligned with data-model-v1.md §5.3.
 *
 * @schema_version 1
 * @source world.schema.json
 */
/** World entity - a narrative universe maintained by creators with timeline evolution. Aligned with data-model-v1.md §5.3. */
export interface World {
  schema_version: number;
  world_id: string;
  owner_creator_id: string;
  title: string;
  slug: string;
  status: WorldStatus;
  visibility: Visibility;
  time_policy: TimePolicy;
  canon_revision?: number;
  current_timeline_head_id?: string;
  current_time_pointer?: string;
  root_fork_branch_id?: string;
  world_rules?: { time_moves_forward?: boolean; history_mutation_requires_fork?: boolean };
  created_at: string;
  updated_at?: string;
}
