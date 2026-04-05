/**
 * Nexus Pairing
 *
 * Pairing entity describing Creator <-> User association. Aligned with data-model-v1.md §5.2A.
 *
 * @schema_version 1
 * @source pairing.schema.json
 */
import type { SchemaVersion } from './CommonTypes';

/** Inline enum type */
export type PairingSource = 'auto_cli' | 'manual_web' | 'platform_auto';

/** Inline enum type */
export type Status = 'active' | 'revoked';

export interface Pairing {
  schema_version: number;
  pairing_id: string;
  creator_id: string;
  user_id: string;
  pairing_source: PairingSource;
  status: Status;
  created_at: string;
  revoked_at?: string;
}
