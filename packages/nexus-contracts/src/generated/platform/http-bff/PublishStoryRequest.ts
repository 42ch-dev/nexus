import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus PublishStoryRequest
 *
 * Request body for POST /v1/publish/story — platform Publish API (display fields, idempotency, chapter selection).
 *
 * @schema_version 1
 * @source publish-story-request.schema.json
 */
/** Request body for POST /v1/publish/story — platform Publish API (display fields, idempotency, chapter selection). */
export interface PublishStoryRequest {
  schema_version: number;
  world_id: string;
  manuscript_id?: string;
  story_manifest_id?: string;
  title: string;
  summary?: string;
  chapter_ids: string[];
  idempotency_key: string;
  sync_command_id?: string;
}
