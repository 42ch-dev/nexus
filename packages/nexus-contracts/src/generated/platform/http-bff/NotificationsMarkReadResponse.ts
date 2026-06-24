import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus NotificationsMarkReadResponse
 *
 * Response for mark-read mutations (platform plan 20).
 *
 * @schema_version 1
 * @source notifications-mark-read-response.schema.json
 */
/** Response for mark-read mutations (platform plan 20). */
export interface NotificationsMarkReadResponse {
  schema_version: number;
  success: boolean;
  updated_count: number;
  error?: string;
}
