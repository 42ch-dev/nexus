import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus OutlinePatchStructureRequest
 *
 * Request body for POST /v1/local/works/{work_id}/outline/patch (V1.72). Mutates the Work outline structure: move a chapter between volumes, attach a chapter to a volume, or link an event to a chapter.
 *
 * @schema_version 1
 * @source outline-patch-structure-request.schema.json
 */

/** Inline enum type */
export type OutlinePatchStructureRequestOperation = 'move_chapter' | 'link_event' | 'attach_to_volume';

/** Request body for POST /v1/local/works/{work_id}/outline/patch (V1.72). Mutates the Work outline structure: move a chapter between volumes, attach a chapter to a volume, or link an event to a chapter. */
export interface OutlinePatchStructureRequest {
  work_id: string;
  base_revision: number;
  operation: OutlinePatchStructureRequestOperation;
  chapter_id?: number;
  volume_id?: number;
  event_id?: string;
  target_chapter_id?: number;
}
