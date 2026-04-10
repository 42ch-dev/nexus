import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus PublishStoryRequest
 *
 * Request body for POST /v1/publish/story — explicit story/manuscript publish (platform Publish API; plan 14 slice).
 *
 * @schema_version 1
 * @source publish-story-request.schema.json
 */
/** Request body for POST /v1/publish/story — explicit story/manuscript publish (platform Publish API; plan 14 slice). */
export interface PublishStoryRequest {
  schema_version: number;
  world_id: string;
  manuscript_id: string;
  story_manifest_id?: string;
}
