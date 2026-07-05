import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListCreatorsQuery
 *
 * Query parameters for GET /v1/daemon/creators.
 *
 * @schema_version 1
 * @source list-creators-query.schema.json
 */
/** Query parameters for GET /v1/daemon/creators. */
export interface ListCreatorsQuery {
  limit?: number;
  cursor?: string;
}
