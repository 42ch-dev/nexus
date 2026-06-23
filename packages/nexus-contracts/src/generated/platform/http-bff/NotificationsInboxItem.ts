import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus NotificationsInboxItem
 *
 * Single inbox notification row (platform plan 20).
 *
 * @schema_version 1
 * @source notifications-inbox-item.schema.json
 */

/** Inline enum type */
export type NotificationsInboxItemKind = 'system' | 'social' | 'publish' | 'workspace' | 'other';

/** Single inbox notification row (platform plan 20). */
export interface NotificationsInboxItem {
  schema_version: number;
  notification_id: string;
  kind: NotificationsInboxItemKind;
  title: string;
  body?: string;
  read_at?: string;
  created_at: string;
  link_url?: string;
}
