/**
 * Nexus OutboxEntry
 *
 * OutboxEntry entity representing a local send queue item. Aligned with data-model-v1.md §5.13.
 *
 * @schema_version 1
 * @source outbox-entry.schema.json
 */
import type { SchemaVersion } from './CommonTypes';

/** Inline enum type */
export type DeliveryState = 'staged' | 'ready' | 'sent' | 'acked' | 'conflicted' | 'failed';

export interface OutboxEntry {
  schema_version: number;
  outbox_entry_id: string;
  bundle_id: string;
  idempotency_key: string;
  delivery_state: DeliveryState;
  retry_count?: number;
  last_error?: string | null;
  next_retry_at?: string;
  created_at: string;
  updated_at?: string;
}
