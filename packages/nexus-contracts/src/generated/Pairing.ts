import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus Pairing
 *
 * Pairing entity describing Creator <-> User association. Aligned with data-model-v1.md §5.2A.
 *
 * @schema_version 1
 * @source pairing.schema.json
 */

/** Inline enum type */
export type PairingPairingSource = 'auto_cli' | 'manual_web' | 'platform_auto';

/** Inline enum type */
export type PairingStatus = 'active' | 'revoked';

/** Pairing entity describing Creator <-> User association. Aligned with data-model-v1.md §5.2A. */
export interface Pairing {
  schema_version: number;
  pairing_id: string;
  creator_id: string;
  user_id: string;
  pairing_source: PairingPairingSource;
  status: PairingStatus;
  created_at: string;
  revoked_at?: string;
}
