/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Owned query for one World's timeline events. Branch defaults to the World's current branch; status defaults to canon; cursor is a bounded opaque keyset over (branch_id, sequence_no).
 */
export interface CoreTimelineEventsQuery {
  /**
   * Fork branch filter; defaults to the World's current branch (fbk_root fallback).
   */
  branch_id?: string;
  /**
   * Event status filter; defaults to canon.
   */
  status?: "canon" | "provisional" | "rejected";
  /**
   * Exact TimelineEventType match.
   */
  event_type?: string;
  /**
   * Page size bound; server applies its default page size when omitted.
   */
  limit?: number;
  /**
   * Opaque keyset cursor from a previous page.
   */
  cursor?: string;
}
