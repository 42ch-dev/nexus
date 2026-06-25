import type { ChapterProtection } from './ChapterProtection';
import type { ChapterStatus } from './ChapterStatus';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ChapterDetailResponse
 *
 * Response for GET /v1/local/works/{work_id}/chapters/{n} (V1.65 P0). Mirrors ChapterSummary plus content metadata. Does not read outline/body content.
 *
 * @schema_version 1
 * @source chapter-detail.schema.json
 */
/** Response for GET /v1/local/works/{work_id}/chapters/{n} (V1.65 P0). Mirrors ChapterSummary plus content metadata. Does not read outline/body content. */
export interface ChapterDetail {
  work_id: string;
  chapter: number;
  volume: number;
  title?: string;
  slug?: string;
  planned_word_count: number;
  actual_word_count?: number;
  status: ChapterStatus;
  outline_path?: string;
  body_path?: string;
  created_at: string;
  updated_at: string;
  can_edit_outline: boolean;
  can_edit_structure: boolean;
  body_read_only: boolean;
  protection: ChapterProtection;
}
