import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus NotificationsListRequest
 *
 * Request body for listing notifications (platform plan 20).
 *
 * @schema_version 1
 * @source notifications-list-request.schema.json
 */
/** Request body for listing notifications (platform plan 20). */
export interface NotificationsListRequest {
  schema_version: number;
  cursor?: string;
  limit?: number;
  unread_only?: boolean;
}
