/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Canonical read model for the Work outline + timeline (V1.72). Exposes the outline_revision and structured metadata needed by the Canvas Outline+Timeline surface.
 */
export interface WorkOutline {
  work_id: string;
  /**
   * Current canonical revision of the Work outline frontmatter.
   */
  outline_revision: number;
  /**
   * Ordered list of volumes, each holding an ordered list of chapter ids.
   */
  volumes: {
    volume_id: number;
    label: string;
    chapter_ids: number[];
  }[];
  /**
   * Timeline events scheduled across chapters.
   */
  timeline_events: {
    event_id: string;
    title: string;
    description?: string;
    realizes_chapter_id?: number;
  }[];
  /**
   * Foreshadow edges linking a source event to a later resolving event.
   */
  foreshadows: {
    source_event_id: string;
    target_event_id: string;
  }[];
  /**
   * UI-facing chapter titles indexed by chapter number string.
   */
  chapter_titles: {
    [k: string]: string | undefined;
  };
  /**
   * ISO 8601 timestamp of the last outline write.
   */
  updated_at: string;
}
