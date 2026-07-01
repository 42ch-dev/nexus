import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReviewRequest
 *
 * Request body for POST /v1/local/memory/review. Triggers the review/summarization pipeline for the active creator's entire pending queue. `creator_id` must match the active creator (config.toml), otherwise 403.
 *
 * @schema_version 1
 * @source review-request.schema.json
 */
/** Request body for POST /v1/local/memory/review. Triggers the review/summarization pipeline for the active creator's entire pending queue. `creator_id` must match the active creator (config.toml), otherwise 403. */
export interface ReviewRequest {
  creator_id: string;
}
