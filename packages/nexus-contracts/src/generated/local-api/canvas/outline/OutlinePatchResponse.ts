import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus OutlinePatchResponse
 *
 * Success response for Canvas Outline+Timeline patch routes (V1.72). Returns the committed revision and any domain validation diagnostics produced during the patch.
 *
 * @schema_version 1
 * @source outline-patch-response.schema.json
 */
/** Success response for Canvas Outline+Timeline patch routes (V1.72). Returns the committed revision and any domain validation diagnostics produced during the patch. */
export interface OutlinePatchResponse {
  new_revision: number;
  validation_summary: { errors: string[]; warnings: string[] };
  side_effects?: string[];
}
