import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus WorkDetailResponse
 *
 * Response for GET /v1/local/works/{work_id} — full work detail.
 *
 * @schema_version 1
 * @source work-detail-response.schema.json
 */
/** Response for GET /v1/local/works/{work_id} — full work detail. */
export interface WorkDetailResponse {
  work_id: string;
  status: string;
  title: string;
  long_term_goal: string;
  initial_idea: string;
  creative_brief?: unknown;
  intake_status: string;
  world_id?: string;
  story_ref?: string;
  inspiration_log: unknown[];
  primary_preset_id: string;
  schedule_ids: string[];
  created_at: string;
  updated_at: string;
  current_stage: string;
  stage_status: string;
  work_profile?: string;
  work_ref?: string;
  total_planned_chapters?: number;
  current_chapter: number;
  chapters?: unknown[];
  next_chapter?: number;
  next_chapter_volume?: number;
  auto_chain_enabled: boolean;
  driver_schedule_id?: string;
  auto_chain_interrupted: boolean;
  auto_review_master_on_timeout: boolean;
  runtime_lock_holder?: string;
  runtime_lock_acquired_at?: string;
  completion_locked_at?: string;
  novel_completion_status?: string;
  lineage_from_work_id?: string;
}
