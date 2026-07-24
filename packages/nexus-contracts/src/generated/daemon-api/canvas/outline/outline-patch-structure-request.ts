/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/works/{work_id}/outline/patch (V1.72). Mutates the Work outline structure: move a chapter between volumes, attach a chapter to a volume, or link an event to a chapter.
 */
export interface OutlinePatchStructureRequest {
  /**
   * Work identifier from the URL path. Must match the path parameter.
   */
  work_id: string;
  /**
   * Revision observed by the client on the last canonical read.
   */
  base_revision: number;
  /**
   * Structural patch operation to perform.
   */
  operation: "move_chapter" | "link_event" | "attach_to_volume";
  /**
   * Target chapter number for move_chapter or attach_to_volume.
   */
  chapter_id?: number;
  /**
   * Destination volume for move_chapter or attach_to_volume.
   */
  volume_id?: number;
  /**
   * Timeline event identifier for link_event.
   */
  event_id?: string;
  /**
   * Chapter that the event realizes for link_event.
   */
  target_chapter_id?: number;
}
