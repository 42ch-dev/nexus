import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus PublishChapterRequest
 *
 * Request body for POST /v1/publish/chapters — publish a single chapter artifact (platform Publish API).
 *
 * @schema_version 1
 * @source publish-chapter-request.schema.json
 */
/** Request body for POST /v1/publish/chapters — publish a single chapter artifact (platform Publish API). */
export interface PublishChapterRequest {
  schema_version: number;
  world_id: string;
  story_manifest_id: string;
  idempotency_key: string;
  title?: string;
  summary?: string;
  sync_command_id?: string;
}
