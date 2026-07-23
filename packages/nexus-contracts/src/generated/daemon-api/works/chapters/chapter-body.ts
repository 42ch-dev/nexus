/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for GET /v1/daemon/works/{work_id}/chapters/{n}/body (V1.65 P0). Body is read-only through this surface.
 */
export interface ChapterBody {
  work_id: string;
  chapter: number;
  volume: number;
  body_path: string;
  /**
   * Full body markdown content. Empty string if body file is missing or empty.
   */
  content: string;
  /**
   * Parsed YAML frontmatter when available; omitted if not parsed by P0.
   */
  frontmatter?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Always true in V1.65.
   */
  read_only: boolean;
  updated_at: string;
}
