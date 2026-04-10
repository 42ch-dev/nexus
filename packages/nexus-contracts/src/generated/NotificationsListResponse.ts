import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus NotificationsListResponse
 *
 * Paginated notifications list (platform plan 20). Item shape matches NotificationsInboxItem fields for wire stability.
 *
 * @schema_version 1
 * @source notifications-list-response.schema.json
 */

/** Inline enum type */
export type NotificationsListResponseItemsKind = 'system' | 'social' | 'publish' | 'workspace' | 'other';

/** Paginated notifications list (platform plan 20). Item shape matches NotificationsInboxItem fields for wire stability. */
export interface NotificationsListResponse {
  schema_version: number;
  items: { notification_id: string; kind: NotificationsListResponseItemsKind; title: string; body?: string; read_at?: string; created_at: string; link_url?: string }[];
  next_cursor?: string;
  has_more: boolean;
}
