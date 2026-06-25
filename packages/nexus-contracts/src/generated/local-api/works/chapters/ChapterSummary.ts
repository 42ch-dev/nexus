import type { ChapterStatus } from './ChapterStatus';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ChapterSummary
 *
 * Summary row for a work chapter in list responses (V1.65 P0). Lightweight — does not read outline/body files.
 *
 * @schema_version 1
 * @source chapter-summary.schema.json
 */
/** Summary row for a work chapter in list responses (V1.65 P0). Lightweight — does not read outline/body files. */
export interface ChapterSummary {
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
}
