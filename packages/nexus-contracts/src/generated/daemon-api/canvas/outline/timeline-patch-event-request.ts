/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/works/{work_id}/timeline/patch (V1.72). Mutates the Work timeline: add, remove, attach to chapter, or create/remove foreshadow links.
 */
export interface TimelinePatchEventRequest {
  /**
   * Work identifier from the URL path. Must match the path parameter.
   */
  work_id: string;
  /**
   * Revision observed by the client on the last canonical read.
   */
  base_revision: number;
  /**
   * Timeline patch operation to perform.
   */
  operation: "add_event" | "remove_event" | "attach_event_to_chapter" | "link_foreshadow" | "unlink_foreshadow";
  /**
   * Identifier of an existing event (required for remove_event, attach_event_to_chapter, link_foreshadow, unlink_foreshadow).
   */
  event_id?: string;
  /**
   * Human-facing title for a new event (add_event).
   */
  title?: string;
  /**
   * Optional longer description for a new event (add_event).
   */
  description?: string;
  /**
   * Chapter number that the event realizes (add_event, attach_event_to_chapter).
   */
  realizes_chapter_id?: number;
  /**
   * Chapter number that the event attaches to (attach_event_to_chapter).
   */
  target_chapter_id?: number;
  /**
   * Event identifier that the source event foreshadows (link_foreshadow) or stops foreshadowing (unlink_foreshadow).
   */
  foreshadows_event_id?: string;
}
