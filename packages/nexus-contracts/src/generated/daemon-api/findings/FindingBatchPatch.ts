import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus FindingBatchPatch
 *
 * Fields to patch on each matching finding in a batch update. At least one field should be present.
 *
 * @schema_version 1
 * @source finding-batch-patch.schema.json
 */
/** Fields to patch on each matching finding in a batch update. At least one field should be present. */
export interface FindingBatchPatch {
  status?: string;
  target_executor?: string;
}
