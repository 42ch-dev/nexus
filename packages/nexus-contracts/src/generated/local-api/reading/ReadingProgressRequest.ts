import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReadingProgressRequest
 *
 * Request body for PUT /v1/local/reading/progress. Upserts persisted scroll position per (creator, work, chapter). Creator scope is inferred from the active session.
 *
 * @schema_version 1
 * @source reading-progress-request.schema.json
 */
/** Request body for PUT /v1/local/reading/progress. Upserts persisted scroll position per (creator, work, chapter). Creator scope is inferred from the active session. */
export interface ReadingProgressRequest {
  work_id: string;
  chapter: number;
  scroll_progress: number;
}
