import type { SchemaVersion } from './CommonTypes';

/**
 * Nexus TimelineEvent
 *
 * TimelineEvent - a canonical event on the world timeline with causality and sequence. Aligned with data-model-v1.md §5.6.
 *
 * @schema_version 1
 * @source timeline-event.schema.json
 */

/** Inline enum type */
export type EventType = 'story_advance' | 'state_update' | 'fork_marker' | 'official_progression' | 'publish_marker';

/** Inline enum type */
export type Status = 'canon' | 'provisional' | 'rejected';

/** TimelineEvent - a canonical event on the world timeline with causality and sequence. Aligned with data-model-v1.md §5.6. */
export interface TimelineEvent {
  schema_version: number;
  timeline_event_id: string;
  world_id: string;
  branch_id: string;
  event_type: EventType;
  status: Status;
  sequence_no: number;
  title?: string;
  summary?: string;
  caused_by_event_ids?: string[];
  affected_key_block_ids?: string[];
  source_command_id?: string;
  created_at: string;
}
