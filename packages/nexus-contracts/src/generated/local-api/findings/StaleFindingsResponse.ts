import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus StaleFindingsResponse
 *
 * Response for GET /v1/local/findings/stale.
 *
 * @schema_version 1
 * @source stale-findings-response.schema.json
 */
/** Response for GET /v1/local/findings/stale. */
export interface StaleFindingsResponse {
  open_count: number;
  stale_threshold_seconds: number;
  items: Record<string, unknown>[];
}
