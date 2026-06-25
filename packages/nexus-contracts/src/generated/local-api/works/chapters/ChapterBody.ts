import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ChapterBody
 *
 * Response body for GET /v1/local/works/{work_id}/chapters/{n}/body (V1.65 P0). Body is read-only through this surface.
 *
 * @schema_version 1
 * @source chapter-body.schema.json
 */
/** Response body for GET /v1/local/works/{work_id}/chapters/{n}/body (V1.65 P0). Body is read-only through this surface. */
export interface ChapterBody {
  work_id: string;
  chapter: number;
  volume: number;
  body_path: string;
  content: string;
  frontmatter?: Record<string, unknown>;
  read_only: boolean;
  updated_at: string;
}
