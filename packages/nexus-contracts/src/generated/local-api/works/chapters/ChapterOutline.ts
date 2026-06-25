import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ChapterOutline
 *
 * Response body for GET/PUT /v1/local/works/{work_id}/chapters/{n}/outline (V1.65 P0).
 *
 * @schema_version 1
 * @source chapter-outline.schema.json
 */
/** Response body for GET/PUT /v1/local/works/{work_id}/chapters/{n}/outline (V1.65 P0). */
export interface ChapterOutline {
  work_id: string;
  chapter: number;
  volume: number;
  outline_path: string;
  content: string;
  updated_at: string;
}
