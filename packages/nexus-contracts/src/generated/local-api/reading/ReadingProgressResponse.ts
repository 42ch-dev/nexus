import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReadingProgressResponse
 *
 * Response for GET and PUT /v1/local/reading/progress. Returns the persisted scroll position for the current creator on the requested (work, chapter). If no progress has been saved, scroll_progress defaults to 0 with a server-generated updated_at.
 *
 * @schema_version 1
 * @source reading-progress-response.schema.json
 */
/** Response for GET and PUT /v1/local/reading/progress. Returns the persisted scroll position for the current creator on the requested (work, chapter). If no progress has been saved, scroll_progress defaults to 0 with a server-generated updated_at. */
export interface ReadingProgressResponse {
  work_id: string;
  chapter: number;
  scroll_progress: number;
  updated_at: string;
}
