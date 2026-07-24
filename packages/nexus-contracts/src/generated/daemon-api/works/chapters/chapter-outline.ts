/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for GET/PUT /v1/daemon/works/{work_id}/chapters/{n}/outline (V1.65 P0).
 */
export interface ChapterOutline {
  work_id: string;
  chapter: number;
  volume: number;
  outline_path: string;
  /**
   * Full outline markdown content. UTF-8.
   */
  content: string;
  updated_at: string;
}
