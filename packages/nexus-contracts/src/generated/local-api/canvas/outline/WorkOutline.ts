import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorkOutline
 *
 * Canonical read model for the Work outline + timeline (V1.72). Exposes the outline_revision and structured metadata needed by the Canvas Outline+Timeline surface.
 *
 * @schema_version 1
 * @source work-outline.schema.json
 */
/** Canonical read model for the Work outline + timeline (V1.72). Exposes the outline_revision and structured metadata needed by the Canvas Outline+Timeline surface. */
export interface WorkOutline {
  work_id: string;
  outline_revision: number;
  volumes: { volume_id: number; label: string; chapter_ids: number[] }[];
  timeline_events: { event_id: string; title: string; description?: string; realizes_chapter_id?: number }[];
  foreshadows: { source_event_id: string; target_event_id: string }[];
  updated_at: string;
}
