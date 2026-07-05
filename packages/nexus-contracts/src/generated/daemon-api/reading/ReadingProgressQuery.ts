import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReadingProgressQuery
 *
 * Query parameters for GET /v1/daemon/reading/progress. Creator scope is inferred from the active session.
 *
 * @schema_version 1
 * @source reading-progress-query.schema.json
 */
/** Query parameters for GET /v1/daemon/reading/progress. Creator scope is inferred from the active session. */
export interface ReadingProgressQuery {
  work_id: string;
  chapter: number;
}
