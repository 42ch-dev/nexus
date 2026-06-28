import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus TimelinePatchEventRequest
 *
 * Request body for POST /v1/local/works/{work_id}/timeline/patch (V1.72). Mutates the Work timeline: add, remove, attach to chapter, or create foreshadow links.
 *
 * @schema_version 1
 * @source timeline-patch-event-request.schema.json
 */

/** Inline enum type */
export type TimelinePatchEventRequestOperation = 'add_event' | 'remove_event' | 'attach_event_to_chapter' | 'link_foreshadow';

/** Request body for POST /v1/local/works/{work_id}/timeline/patch (V1.72). Mutates the Work timeline: add, remove, attach to chapter, or create foreshadow links. */
export interface TimelinePatchEventRequest {
  work_id: string;
  base_revision: number;
  operation: TimelinePatchEventRequestOperation;
  event_id?: string;
  title?: string;
  description?: string;
  realizes_chapter_id?: number;
  target_chapter_id?: number;
  foreshadows_event_id?: string;
}
