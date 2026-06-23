import type { PairingSource, PairingStatus, SchemaVersion } from '../common/CommonTypes';
/**
 * Nexus Pairing
 *
 * Pairing entity describing Creator <-> User association. Aligned with data-model-v1.md §5.2A.
 *
 * @schema_version 1
 * @source pairing.schema.json
 */
/** Pairing entity describing Creator <-> User association. Aligned with data-model-v1.md §5.2A. */
export interface Pairing {
  schema_version: number;
  pairing_id: string;
  creator_id: string;
  user_id: string;
  pairing_source: PairingSource;
  status: PairingStatus;
  created_at: string;
  revoked_at?: string;
}
