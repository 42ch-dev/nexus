/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Timeline event row for a world's per-branch timeline (GET /v1/daemon/worlds/:world_id/timeline/events). Maps 1:1 to a narrative_timeline_events row: typed columns stay authoritative, extensions carries the parsed extensions_nexus_json namespace (compute provenance etc.).
 */
export interface TimelineEventInfo {
  /**
   * Timeline event identifier (timeline_event_id).
   */
  id: string;
  /**
   * Fork branch ID this event belongs to (root branch or specific fork).
   */
  branch_id: string;
  /**
   * Event type. Machine-written log families (e.g. compute_result) and author event types share this column.
   */
  event_type: string;
  /**
   * Event status; only canon is merged into the Narrative layer projection.
   */
  status: "canon" | "provisional" | "rejected";
  /**
   * Append-ordered sequence number within (world_id, branch_id).
   */
  sequence_no: number;
  /**
   * Event title.
   */
  title?: string | null;
  /**
   * Event summary.
   */
  summary?: string | null;
  /**
   * KnowledgeEntry entry_ids affected by this event.
   */
  affected_key_block_ids?: string[] | null;
  /**
   * Timeline event ids that caused this one.
   */
  caused_by_event_ids?: string[] | null;
  /**
   * SyncCommand that triggered this event.
   */
  source_command_id?: string | null;
  /**
   * Structured event metadata (metadata_json column).
   */
  metadata: {
    [k: string]: unknown | undefined;
  };
  /**
   * Parsed extensions_nexus_json namespace — carries compute provenance (module_id, module_version, run_id, source_kind) and any future extension keys.
   */
  extensions?: {
    [k: string]: unknown | undefined;
  } | null;
  /**
   * Per-event functional-dialect modules (modules.observation) carried verbatim from narrative_timeline_events.modules_json. Absent when unrecorded (V1.164 P3, AR-2).
   */
  modules?: {
    [k: string]: unknown | undefined;
  };
  /**
   * ISO 8601 UTC timestamp of event creation.
   */
  created_at: string;
}
