import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus NotificationsMarkReadRequest
 *
 * Request body for marking notifications read (platform plan 20). Either pass explicit ids or mark_all.
 *
 * @schema_version 1
 * @source notifications-mark-read-request.schema.json
 */
/** Request body for marking notifications read (platform plan 20). Either pass explicit ids or mark_all. */
export interface NotificationsMarkReadRequest {
  schema_version: number;
  notification_ids?: string[];
  mark_all?: boolean;
}
