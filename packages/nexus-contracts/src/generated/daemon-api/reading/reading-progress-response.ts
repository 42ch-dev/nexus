/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET and PUT /v1/daemon/reading/progress. Returns the persisted scroll position for the current creator on the requested (work, chapter). If no progress has been saved, scroll_progress defaults to 0 with a server-generated updated_at.
 */
export interface ReadingProgressResponse {
  work_id: string;
  chapter: number;
  scroll_progress: number;
  /**
   * ISO 8601 timestamp of the last progress save.
   */
  updated_at: string;
}
