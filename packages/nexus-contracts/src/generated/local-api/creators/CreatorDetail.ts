import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreatorDetail
 *
 * Response for GET /v1/local/creators/{creator_id}.
 *
 * @schema_version 1
 * @source creator-detail.schema.json
 */
/** Response for GET /v1/local/creators/{creator_id}. */
export interface CreatorDetail {
  creator_id: string;
  handle?: string;
  display_name?: string;
  has_api_key: boolean;
  has_cached_token: boolean;
  is_active: boolean;
}
