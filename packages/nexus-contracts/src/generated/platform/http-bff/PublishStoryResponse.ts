import type { PublishStoryOutcome, SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus PublishStoryResponse
 *
 * Response body for POST /v1/publish/story.
 *
 * @schema_version 1
 * @source publish-story-response.schema.json
 */
/** Response body for POST /v1/publish/story. */
export interface PublishStoryResponse {
  schema_version: number;
  outcome: PublishStoryOutcome;
  message?: string;
  published_artifact_id?: string;
  error_code?: string;
}
