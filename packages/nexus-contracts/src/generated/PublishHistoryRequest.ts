import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus PublishHistoryRequest
 *
 * Request body for POST /v1/publish/history — paginated publish history with optional filters (platform API).
 *
 * @schema_version 1
 * @source publish-history-request.schema.json
 */

/** Inline enum type */
export type PublishHistoryRequestArtifactType = 'chapter' | 'story';

/** Request body for POST /v1/publish/history — paginated publish history with optional filters (platform API). */
export interface PublishHistoryRequest {
  schema_version: number;
  world_id?: string;
  manuscript_id?: string;
  artifact_type?: PublishHistoryRequestArtifactType;
  cursor?: string;
  limit?: number;
}
