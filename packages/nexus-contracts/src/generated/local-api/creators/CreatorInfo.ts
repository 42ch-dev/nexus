import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus CreatorInfo
 *
 * Creator info row from local identity store.
 *
 * @schema_version 1
 * @source creator-info.schema.json
 */
/** Creator info row from local identity store. */
export interface CreatorInfo {
  creator_id: string;
  display_name: string;
  status: string;
  cached_at?: string;
}
