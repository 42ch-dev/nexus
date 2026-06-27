import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus StrategyPatchResponse
 *
 * Success response for Strategy patch routes (V1.71). Returns the committed revision and any domain validation diagnostics produced during the patch.
 *
 * @schema_version 1
 * @source strategy-patch-response.schema.json
 */
/** Success response for Strategy patch routes (V1.71). Returns the committed revision and any domain validation diagnostics produced during the patch. */
export interface StrategyPatchResponse {
  new_revision: number;
  validation_summary: { errors: string[]; warnings: string[] };
  side_effects?: string[];
}
