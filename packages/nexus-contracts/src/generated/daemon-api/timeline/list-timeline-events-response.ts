/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { TimelineEventInfo } from './timeline-event-info';

/**
 * Response for GET /v1/daemon/worlds/:world_id/timeline/events — cursor-paginated list of timeline events for a world (branch/status/event_type filters, keyset cursor on (branch_id, sequence_no)).
 */
export interface ListTimelineEventsResponse {
  /**
   * TimelineEventInfo rows, ordered by (branch_id, sequence_no) ascending within the filtered branch.
   */
  items: TimelineEventInfo[];
  /**
   * True when another page exists (equivalent to next_cursor being non-null).
   */
  has_more: boolean;
  /**
   * Opaque keyset cursor for the next page. Clients MUST NOT parse it. Non-null only when has_more is true.
   */
  next_cursor?: string;
}
