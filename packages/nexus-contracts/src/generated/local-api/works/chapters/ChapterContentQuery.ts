import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ChapterContentQuery
 *
 * Query parameters for GET/PUT/PATCH chapter detail, outline, and body routes (V1.65 P0).
 *
 * @schema_version 1
 * @source chapter-content-query.schema.json
 */
/** Query parameters for GET/PUT/PATCH chapter detail, outline, and body routes (V1.65 P0). */
export interface ChapterContentQuery {
  volume?: number;
}
