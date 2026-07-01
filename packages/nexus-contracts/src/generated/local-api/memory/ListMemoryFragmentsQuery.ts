import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListMemoryFragmentsQuery
 *
 * Query parameters for GET /v1/local/memory/fragments. `keyword` is an optional case-insensitive LIKE filter; `limit` defaults to 50 (clamped 1..=250) when omitted.
 *
 * @schema_version 1
 * @source list-memory-fragments-query.schema.json
 */
/** Query parameters for GET /v1/local/memory/fragments. `keyword` is an optional case-insensitive LIKE filter; `limit` defaults to 50 (clamped 1..=250) when omitted. */
export interface ListMemoryFragmentsQuery {
  creator_id: string;
  keyword?: string;
  limit?: number;
}
