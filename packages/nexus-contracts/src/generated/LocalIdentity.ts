import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus Local Creator Identity
 *
 * Local-only creator identity for local_only mode. Supports anonymous (ephemeral) and persistent identities without platform dependency. See ADR-017, ADR-014.
 *
 * @schema_version 1
 * @source local-identity.schema.json
 */

/** Inline enum type */
export type LocalIdentityIdentityType = 'anonymous' | 'persistent';

/** Local-only creator identity for local_only mode. Supports anonymous (ephemeral) and persistent identities without platform dependency. See ADR-017, ADR-014. */
export interface LocalIdentity {
  schema_version: number;
  creator_id: string;
  identity_type: LocalIdentityIdentityType;
  display_name?: string;
  created_at: string;
  platform_linked: boolean;
  platform_creator_id?: string;
}
