import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReadingAnnotationListQuery
 *
 * Query parameters for GET /v1/daemon/reading/annotations. Returns all annotations for the current creator on a given (work, chapter). Creator scope is inferred from the active session.
 *
 * @schema_version 1
 * @source reading-annotation-list-query.schema.json
 */
/** Query parameters for GET /v1/daemon/reading/annotations. Returns all annotations for the current creator on a given (work, chapter). Creator scope is inferred from the active session. */
export interface ReadingAnnotationListQuery {
  work_id: string;
  chapter: number;
}
