import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SocialGraphRelationshipResponse
 *
 * Response envelope for social graph mutation endpoints (platform plan 17).
 *
 * @schema_version 1
 * @source social-graph-relationship-response.schema.json
 */
/** Response envelope for social graph mutation endpoints (platform plan 17). */
export interface SocialGraphRelationshipResponse {
  schema_version: number;
  success: boolean;
  following?: boolean;
  favorited?: boolean;
  error?: string;
}
