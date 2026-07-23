/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Lifecycle status of a work chapter (V1.65 P0).
 */
export type NexusChapterStatus = "not_started" | "outlined" | "draft" | "finalized" | "published";

/**
 * Response for GET /v1/daemon/works/{work_id}/chapters (V1.65 P0). Cursor-based pagination over ChapterSummary rows. Uses `items` key per F-P3.
 */
export interface ListChaptersResponse {
  items: NexusChapterSummary[];
  pagination: NexusPaginationInfo;
}
/**
 * Summary row for a work chapter in list responses (V1.65 P0). Lightweight — does not read outline/body files.
 */
export interface NexusChapterSummary {
  work_id: string;
  chapter: number;
  volume: number;
  /**
   * Human title if materialized by P0; otherwise clients may derive display text from slug/chapter number. V1.65 returns null.
   */
  title?: string;
  slug?: string;
  planned_word_count: number;
  actual_word_count?: number;
  status: NexusChapterStatus;
  /**
   * Relative path to outline file, or empty string if not initialized.
   */
  outline_path?: string;
  /**
   * Relative path to body file, or empty string if not initialized. Body is read-only.
   */
  body_path?: string;
  created_at: string;
  updated_at: string;
}
/**
 * Cursor-based pagination metadata.
 */
export interface NexusPaginationInfo {
  limit: number;
  /**
   * Opaque cursor returned by the previous page. Clients MUST NOT parse it. Non-null only when another page exists.
   */
  next_cursor?: string;
  /**
   * True when the client may request another page (equivalent to `next_cursor` being non-null).
   */
  has_more: boolean;
}
