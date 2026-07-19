import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus TimelineOverviewResponse
 *
 * Cursor-paginated overview of visible Worlds with per-World era/event counts and last activity timestamp. Response for GET /v1/daemon/timeline/overview.
 *
 * @schema_version 1
 * @source timeline-overview-response.schema.json
 */
/** Cursor-paginated overview of visible Worlds with per-World era/event counts and last activity timestamp. Response for GET /v1/daemon/timeline/overview. */
export interface TimelineOverviewResponse {
  worlds: { world_id: string; title?: string | null; era_count: number; event_count: number; last_event_at?: string | null }[];
  cursor?: string | null;
  total_worlds: number;
}
